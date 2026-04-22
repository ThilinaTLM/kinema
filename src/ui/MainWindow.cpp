// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/MainWindow.h"

#include "api/CinemetaClient.h"
#include "api/TmdbClient.h"
#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "controllers/MovieDetailController.h"
#include "controllers/NavigationController.h"
#include "controllers/Page.h"
#include "controllers/SearchController.h"
#include "controllers/SeriesDetailController.h"
#include "controllers/TokenController.h"
#include "controllers/TrayController.h"
#include "services/StreamActions.h"
#include "core/DateFormat.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "core/PlayerLauncher.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"
#include "ui/BrowsePage.h"
#include "ui/DetailPane.h"
#include "ui/DiscoverPage.h"
#include "ui/ImageLoader.h"
#include "ui/ResultCardDelegate.h"
#include "ui/ResultsModel.h"
#include "ui/SearchBar.h"
#include "ui/SeriesDetailPane.h"
#include "ui/StateWidget.h"
#include "ui/settings/SettingsDialog.h"

#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/PlayerWindow.h"
#endif

#include <QEvent>

#include <KNotification>

#include <KAboutApplicationDialog>
#include <KAboutData>
#include <KActionCollection>
#include <KHamburgerMenu>
#include <KLocalizedString>
#include <KStandardAction>

#include <QAction>
#include <QApplication>
#include <QCloseEvent>
#include <QIcon>
#include <QKeySequence>
#include <QListView>
#include <QMenu>
#include <QMenuBar>
#include <QPalette>
#include <QShortcut>
#include <QStackedWidget>
#include <QStatusBar>
#include <QTimer>
#include <QToolBar>
#include <QVBoxLayout>

namespace kinema::ui {

MainWindow::MainWindow(config::AppSettings& settings, QWidget* parent)
    : QMainWindow(parent)
    , m_settings(settings)
{
    setWindowTitle(QStringLiteral("Kinema"));
    setWindowIcon(QIcon::fromTheme(QStringLiteral("dev.tlmtech.kinema")));
    resize(1200, 780);

    buildCoreServices();
    buildLayout();
    buildActions();
    wireStatusSignals();
    wireRealDebridState();
    wireTray();

    // Fire the initial keyring reads (RD + TMDB). Does not block.
    m_tokenCtrl->loadAll();
}

MainWindow::~MainWindow() = default;

void MainWindow::buildCoreServices()
{
    m_http = std::make_unique<core::HttpClient>(this);
    m_tokens = std::make_unique<core::TokenStore>(this);
    m_player = std::make_unique<core::PlayerLauncher>(
        m_settings.player(), this);
    m_cinemeta = new api::CinemetaClient(m_http.get(), this);
    m_torrentio = new api::TorrentioClient(m_http.get(), this);
    m_tmdb = new api::TmdbClient(m_http.get(), this);
    m_imageLoader = new ImageLoader(m_http.get(), this);
    m_streamActions = new services::StreamActions(m_player.get(), this);
}

void MainWindow::wireStatusSignals()
{
    // Mirror PlayerLauncher results into the status bar so the user
    // gets a lightweight in-window confirmation on top of the
    // KNotification the launcher already fires.
    connect(m_player.get(), &core::PlayerLauncher::launched, this,
        [this](core::player::Kind, const QString& title) {
            statusBar()->showMessage(
                i18nc("@info:status", "Playing: %1", title), 4000);
        });
    connect(m_player.get(), &core::PlayerLauncher::launchFailed, this,
        [this](core::player::Kind, const QString& reason) {
            statusBar()->showMessage(reason, 6000);
        });
    connect(m_streamActions, &services::StreamActions::statusMessage,
        this, [this](const QString& text, int timeoutMs) {
            statusBar()->showMessage(text, timeoutMs);
        });
#ifdef KINEMA_HAVE_LIBMPV
    connect(m_player.get(), &core::PlayerLauncher::embeddedRequested,
        this, &MainWindow::openEmbeddedPlayer);
#endif

    if (!m_player->preferredPlayerAvailable()) {
        qCInfo(KINEMA) << "preferred media player not found on $PATH";
    }
}

void MainWindow::wireRealDebridState()
{
    // RD state is driven by RealDebridSettings ("configured" bit) +
    // the in-memory token owned by TokenController.
    const bool hasRD = m_settings.realDebrid().configured();
    m_detailPane->setRealDebridConfigured(hasRD);
    m_seriesDetailPane->setRealDebridConfigured(hasRD);
    connect(&m_settings.realDebrid(),
        &config::RealDebridSettings::configuredChanged,
        this, [this](bool on) {
            m_detailPane->setRealDebridConfigured(on);
            m_seriesDetailPane->setRealDebridConfigured(on);
        });

    // Refetch visible torrents whenever a Torrentio filter/sort default
    // changes. Multiple setters in one Apply click emit the signal
    // multiple times; debounce via a single-shot.
    connect(&m_settings,
        &config::AppSettings::torrentioOptionsChanged,
        this, &MainWindow::onTorrentioOptionsChanged);
}

void MainWindow::wireTray()
{
    // System tray. No-op on desktops that don't expose a tray; the
    // close-to-tray path silently falls back to "close = quit" there.
    m_tray = new controllers::TrayController(this, this);
    connect(m_tray,
        &controllers::TrayController::toggleMainWindowRequested,
        this, &MainWindow::toggleMainWindow);
    connect(m_tray,
        &controllers::TrayController::showPlayerWindowRequested,
        this, [this] {
#ifdef KINEMA_HAVE_LIBMPV
            if (m_playerWindow) {
                m_playerWindow->show();
                m_playerWindow->raise();
                m_playerWindow->activateWindow();
                m_tray->refreshMenu();
            }
#endif
        });
    connect(m_tray, &controllers::TrayController::quitRequested,
        this, &MainWindow::quitApplication);
}

void MainWindow::buildLayout()
{
    buildSearchSurface();
    buildResultsPages();
    buildDetailPages();
    buildControllers();
    statusBar()->showMessage(i18nc("@info:status", "Ready"), 2000);
}

void MainWindow::buildSearchSurface()
{
    // ---- Search bar in a tool bar ----------------------------------------
    m_toolbar = addToolBar(i18nc("@title:window", "Search"));
    m_toolbar->setMovable(false);
    m_toolbar->setFloatable(false);
    m_toolbar->setContextMenuPolicy(Qt::PreventContextMenu);
    m_searchBar = new SearchBar(m_toolbar);
    m_searchBar->setMediaKind(m_settings.search().kind());
    m_toolbar->addWidget(m_searchBar);
    connect(m_searchBar, &SearchBar::queryRequested,
        this, &MainWindow::onQueryRequested);
    connect(m_searchBar, &SearchBar::mediaKindChanged, this,
        [this](api::MediaKind kind) {
            m_settings.search().setKind(kind);
        });

    // ---- Results view ----------------------------------------------------
    m_resultsModel = new ResultsModel(this);
    m_resultsView = new QListView(this);
    // Blend the results grid viewport with the surrounding window
    // chrome instead of the darker QPalette::Base default.
    m_resultsView->viewport()->setBackgroundRole(QPalette::Window);
    m_resultsView->setModel(m_resultsModel);
    m_resultsView->setViewMode(QListView::IconMode);
    m_resultsView->setResizeMode(QListView::Adjust);
    m_resultsView->setMovement(QListView::Static);
    m_resultsView->setUniformItemSizes(true);
    m_resultsView->setSpacing(8);
    m_resultsView->setWordWrap(true);
    m_resultsView->setSelectionMode(QAbstractItemView::SingleSelection);
    m_resultsView->setMouseTracking(true);

    m_resultsDelegate = new ResultCardDelegate(m_imageLoader, m_resultsView);
    m_resultsView->setItemDelegate(m_resultsDelegate);

    connect(m_resultsView, &QListView::activated,
        this, &MainWindow::onResultActivated);
    connect(m_resultsView, &QListView::clicked,
        this, &MainWindow::onResultActivated);

    m_resultsState = new StateWidget(this);
    m_resultsState->showIdle(
        i18nc("@label", "Search to get started"),
        i18nc("@info", "Type a movie title or an IMDB id and press Enter."));
}

void MainWindow::buildResultsPages()
{
    // Discover home page (TMDB-powered catalog strips). Sits at index
    // 0 so it's the app's default landing surface; runSearch() swaps
    // to m_resultsState / m_resultsView; showDiscoverHome() swaps
    // back.
    m_discoverPage = new DiscoverPage(m_tmdb, m_imageLoader, this);
    connect(m_discoverPage, &DiscoverPage::itemActivated,
        this, &MainWindow::onDiscoverItemActivated);
    connect(m_discoverPage, &DiscoverPage::settingsRequested,
        this, &MainWindow::showSettings);

    // Browse page (TMDB /discover with filters). Added to the
    // results stack alongside Discover / search state / search
    // results so Back naturally walks Browse → Discover.
    m_browsePage = new BrowsePage(
        m_tmdb, m_imageLoader, m_settings.browse(), this);
    connect(m_browsePage, &BrowsePage::itemActivated,
        this, &MainWindow::onDiscoverItemActivated);
    connect(m_browsePage, &BrowsePage::settingsRequested,
        this, &MainWindow::showSettings);

    m_resultsStack = new QStackedWidget(this);
    m_resultsStack->addWidget(m_discoverPage); // idx 0 = home
    m_resultsStack->addWidget(m_resultsState); // idx 1 = search state
    m_resultsStack->addWidget(m_resultsView);  // idx 2 = search results
    m_resultsStack->addWidget(m_browsePage);   // idx 3 = browse
}

void MainWindow::buildDetailPages()
{
    m_detailPane = new DetailPane(m_imageLoader, m_tmdb,
        m_settings.torrentio(), m_settings.filter(),
        *m_streamActions, m_settings.appearance(), this);
    connect(m_detailPane, &DetailPane::similarActivated,
        this, &MainWindow::onDiscoverItemActivated);

    m_seriesDetailPane = new SeriesDetailPane(m_imageLoader, m_tmdb,
        m_settings.torrentio(), m_settings.filter(),
        *m_streamActions, m_settings.appearance(), this);
    connect(m_seriesDetailPane, &SeriesDetailPane::similarActivated,
        this, &MainWindow::onDiscoverItemActivated);
    connect(m_seriesDetailPane, &SeriesDetailPane::episodeSelected,
        this, &MainWindow::onEpisodeSelected);
    connect(m_seriesDetailPane, &SeriesDetailPane::backToEpisodesRequested,
        this, &MainWindow::onBackToEpisodes);

    m_centerStack = new QStackedWidget(this);
    m_centerStack->addWidget(m_resultsStack);       // idx 0 = results
    m_centerStack->addWidget(m_detailPane);         // idx 1 = movie
    m_centerStack->addWidget(m_seriesDetailPane);   // idx 2 = series
    m_centerStack->setCurrentWidget(m_resultsStack);

    setCentralWidget(m_centerStack);
}

void MainWindow::buildControllers()
{
    // Navigation state machine. Owns "what page is visible" and the
    // Back rules; every detail/results/back call below routes through
    // nav->goTo() / nav->goBack().
    m_nav = new controllers::NavigationController(
        m_centerStack, m_resultsStack,
        m_discoverPage, m_browsePage, m_resultsState, m_resultsView,
        m_detailPane, m_seriesDetailPane, this);
    connect(m_nav, &controllers::NavigationController::currentChanged,
        this, [this](controllers::Page) { updateBackActionEnabled(); });

    // Search coroutine + epoch + state routing.
    m_search = new controllers::SearchController(
        m_cinemeta, m_resultsModel, m_resultsState, m_nav, this);
    connect(m_search, &controllers::SearchController::queryStarted,
        this, [this] {
            if (m_resultsDelegate) {
                m_resultsDelegate->resetRequestTracking();
            }
            closeDetailPanel();
        });
    connect(m_search, &controllers::SearchController::resultsReady,
        this, [this](int) { m_resultsView->setFocus(); });
    connect(m_search, &controllers::SearchController::statusMessage,
        this, [this](const QString& text, int timeoutMs) {
            statusBar()->showMessage(text, timeoutMs);
        });

    // TokenController owns the in-memory RD + TMDB tokens. It must
    // outlive the detail controllers below because they alias its
    // realDebridToken() by const reference.
    m_tokenCtrl = new controllers::TokenController(
        m_tokens.get(), m_tmdb, m_settings.realDebrid(), this);
    connect(m_tokenCtrl,
        &controllers::TokenController::tmdbTokenChanged,
        this, [this](const QString& token) {
            if (m_discoverPage) {
                if (token.isEmpty()) {
                    m_discoverPage->showNotConfigured();
                } else {
                    m_discoverPage->refresh();
                }
            }
            if (m_browsePage) {
                // Browse is lazy — don't refetch until the user visits
                // it. The CTA mirrors the token state so the first
                // visit lands on the right page.
                if (token.isEmpty()) {
                    m_browsePage->showNotConfigured();
                } else if (m_nav
                    && m_nav->current() == controllers::Page::Browse) {
                    m_browsePage->refresh();
                }
            }
        });

    // Movie + series detail coroutines. Both receive the RD token
    // by reference so token updates land immediately without a
    // refetch.
    m_movieCtrl = new controllers::MovieDetailController(
        m_cinemeta, m_torrentio, m_tmdb, m_detailPane, m_nav,
        m_settings, m_tokenCtrl->realDebridToken(), this);
    m_seriesCtrl = new controllers::SeriesDetailController(
        m_cinemeta, m_torrentio, m_tmdb, m_seriesDetailPane, m_nav,
        m_settings, m_tokenCtrl->realDebridToken(), this);
    const auto forwardStatus =
        [this](const QString& text, int timeoutMs) {
            statusBar()->showMessage(text, timeoutMs);
        };
    connect(m_movieCtrl,
        &controllers::MovieDetailController::statusMessage,
        this, forwardStatus);
    connect(m_seriesCtrl,
        &controllers::SeriesDetailController::statusMessage,
        this, forwardStatus);
}

void MainWindow::showMovieDetail()
{
    if (m_nav) {
        m_nav->goTo(controllers::Page::MovieDetail);
    }
}

void MainWindow::showSeriesDetail()
{
    if (m_nav) {
        m_nav->goTo(controllers::Page::SeriesEpisodes);
    }
}

void MainWindow::closeDetailPanel()
{
    if (!m_nav || !controllers::isDetailPage(m_nav->current())) {
        return;
    }

    // Cancel any in-flight work so stale fetches don't repopulate the
    // pane after it's been dismissed.
    m_movieCtrl->clear();
    m_seriesCtrl->clear();

    // Reset both panes to their idle states so re-opening a different
    // kind later doesn't momentarily flash stale content.
    m_detailPane->showIdle();
    m_seriesDetailPane->showMetaLoading();

    // goTo returns us to whatever results-stack page the nav remembered
    // from before the detail was opened.
    m_nav->goTo(m_nav->resultsPageBeforeDetail());

    // Clear selection so clicking the same card re-opens cleanly.
    if (auto* sm = m_resultsView->selectionModel()) {
        sm->clearSelection();
        sm->clearCurrentIndex();
    }
    if (m_nav && m_nav->current() == controllers::Page::SearchResults) {
        m_resultsView->setFocus();
    }

    setWindowTitle(QStringLiteral("Kinema"));
}

#ifdef KINEMA_HAVE_LIBMPV

void MainWindow::openEmbeddedPlayer(const QUrl& url, const QString& title)
{
    // Lazy construction: the MpvWidget (inside PlayerWindow)
    // initialises libmpv + an OpenGL render context in its ctor /
    // initializeGL, so we pay nothing until the user actually picks
    // "Embedded" and hits Play.
    if (!m_playerWindow) {
        m_playerWindow = new player::PlayerWindow(
            m_settings.appearance(), this);
        // Let the tray controller surface a "Show Player" entry
        // whenever the player is hidden but has played something.
        m_tray->setPlayerWindow(m_playerWindow);

        connect(m_playerWindow, &player::PlayerWindow::fileLoaded,
            this, [this] {
                auto* n = new KNotification(
                    QStringLiteral("playbackStarted"),
                    KNotification::CloseOnTimeout);
                n->setTitle(i18nc("@title:window notification",
                    "Playing in Kinema"));
                // m_playerWindow's title is already "<media> — Kinema".
                n->setText(m_playerWindow->windowTitle());
                n->setIconName(
                    QStringLiteral("media-playback-start"));
                n->sendEvent();
            });
        connect(m_playerWindow, &player::PlayerWindow::mpvError,
            this, [this](const QString& reason) {
                statusBar()->showMessage(reason, 6000);
                auto* n = new KNotification(
                    QStringLiteral("playbackFailed"),
                    KNotification::CloseOnTimeout);
                n->setTitle(i18nc("@title:window notification",
                    "Embedded playback failed"));
                n->setText(reason);
                n->setIconName(QStringLiteral("dialog-error"));
                n->sendEvent();
            });
        connect(m_playerWindow, &player::PlayerWindow::endOfFile,
            this, [this](const QString& reason) {
                // PlayerWindow has already stopped playback and
                // hidden itself by now — just surface an error
                // status for abnormal exits.
                if (reason == QLatin1String("error")) {
                    statusBar()->showMessage(
                        i18nc("@info:status",
                            "Playback ended with an error."),
                        6000);
                }
            });
        connect(m_playerWindow, &player::PlayerWindow::visibilityChanged,
            this, [this](bool) { m_tray->refreshMenu(); });
    }

    statusBar()->showMessage(
        i18nc("@info:status", "Loading “%1”…", title), 3000);

    m_playerWindow->play(url, title);
}

#endif // KINEMA_HAVE_LIBMPV

void MainWindow::onBackToEpisodes()
{
    // Delegate to SeriesDetailController — it owns the episode epoch
    // and syncs nav when the back path came from the in-pane button.
    m_seriesCtrl->onBackToEpisodes();
}

void MainWindow::onBackRequested()
{
    // The embedded player lives in its own top-level window and owns
    // its own Back / Esc handling, so MainWindow's back action only
    // walks the main-window nav stack.
    if (!m_nav) {
        return;
    }

    const auto before = m_nav->current();

    // Intercept the two transitions that need MainWindow-side work:
    //   SeriesStreams → SeriesEpisodes  — cancel in-flight streams
    //                                     (onBackToEpisodes flips nav)
    //   Detail → results             — drop detail epochs + state
    //                                     (closeDetailPanel flips nav)
    if (before == controllers::Page::SeriesStreams) {
        onBackToEpisodes();
        return;
    }
    if (controllers::isDetailPage(before)) {
        closeDetailPanel();
        return;
    }
    m_nav->goBack();
}

void MainWindow::updateBackActionEnabled()
{
    if (!m_backAction) {
        return;
    }
    m_backAction->setEnabled(m_nav && m_nav->canGoBack());
}

void MainWindow::buildActions()
{
    m_actions = new KActionCollection(this);

    // ---- Standard actions ------------------------------------------------
    auto* quitAction = KStandardAction::quit(
        this, &MainWindow::quitApplication, m_actions);
    auto* prefsAction = KStandardAction::preferences(
        this, &MainWindow::showSettings, m_actions);
    auto* aboutAction = KStandardAction::aboutApp(
        this, &MainWindow::showAbout, m_actions);
    aboutAction->setIcon(QIcon::fromTheme(QStringLiteral("help-about"),
        QIcon::fromTheme(QStringLiteral("dev.tlmtech.kinema"))));
    m_showMenubarAction = KStandardAction::showMenubar(
        this, &MainWindow::onShowMenubarToggled, m_actions);

    // ---- Custom actions --------------------------------------------------
    auto* focusSearch = new QAction(
        QIcon::fromTheme(QStringLiteral("search")),
        i18nc("@action:inmenu", "&Focus search"), this);
    focusSearch->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_K));
    connect(focusSearch, &QAction::triggered, this, [this] {
        m_searchBar->setFocus();
    });
    m_actions->addAction(QStringLiteral("focus_search"), focusSearch);

    auto* homeAction = KStandardAction::home(
        this, &MainWindow::showDiscoverHome, m_actions);
    homeAction->setToolTip(i18nc("@info:tooltip", "Back to Discover"));

    m_browseAction = new QAction(
        QIcon::fromTheme(QStringLiteral("view-list-icons")),
        i18nc("@action:inmenu", "&Browse"), this);
    m_browseAction->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_B));
    m_browseAction->setToolTip(i18nc("@info:tooltip",
        "Browse movies and TV with filters (Ctrl+B)"));
    connect(m_browseAction, &QAction::triggered,
        this, &MainWindow::showBrowsePage);
    m_actions->addAction(QStringLiteral("browse_page"), m_browseAction);

    m_backAction = new QAction(
        QIcon::fromTheme(QStringLiteral("go-previous")),
        i18nc("@action:inmenu", "&Back"), this);
    m_backAction->setShortcut(QKeySequence(Qt::ALT | Qt::Key_Left));
    m_backAction->setToolTip(i18nc("@info:tooltip", "Go back"));
    m_backAction->setWhatsThis(i18nc("@info:whatsthis",
        "Walk one step back through the navigation: streams → episodes, "
        "detail → results, search → Discover."));
    connect(m_backAction, &QAction::triggered,
        this, &MainWindow::onBackRequested);
    m_actions->addAction(QStringLiteral("go_back"), m_backAction);

    buildEscapeShortcut();
    buildMenuBar(quitAction, focusSearch, homeAction, prefsAction, aboutAction);
    auto* hamburger = buildHamburgerMenu(
        quitAction, focusSearch, homeAction, prefsAction, aboutAction);
    buildToolbarActions(homeAction, hamburger);

    // Bind all actions (and their shortcuts) to the main window so keys
    // like Ctrl+M / Ctrl+Q / Ctrl+K fire regardless of menu-bar state.
    m_actions->addAssociatedWidget(this);

    restoreMenubarVisibility();
    updateBackActionEnabled();
}

void MainWindow::buildEscapeShortcut()
{
    // Esc anywhere in the window walks back one navigation level:
    //   streams page → episode list, then detail view → results.
    // No modal flows live inside the main window (Settings is a
    // top-level dialog), so a window-scope shortcut is safe.
    auto* closePanelShortcut = new QShortcut(QKeySequence(Qt::Key_Escape), this);
    closePanelShortcut->setContext(Qt::WindowShortcut);
    connect(closePanelShortcut, &QShortcut::activated, this, [this] {
        // The embedded player window handles its own Esc key
        // (fullscreen → windowed → close); this shortcut only fires
        // when the main window has focus, so we just drive the
        // main-window nav stack.
        if (!m_nav) {
            return;
        }
        if (m_nav->current() == controllers::Page::SeriesStreams) {
            onBackToEpisodes();
            return;
        }
        if (controllers::isDetailPage(m_nav->current())) {
            closeDetailPanel();
        }
    });
}

void MainWindow::buildMenuBar(QAction* quitAction,
    QAction* focusSearch,
    QAction* homeAction,
    QAction* prefsAction,
    QAction* aboutAction)
{
    auto* fileMenu = menuBar()->addMenu(i18nc("@title:menu", "&File"));
    fileMenu->addAction(quitAction);

    auto* goMenu = menuBar()->addMenu(i18nc("@title:menu", "&Go"));
    goMenu->addAction(m_backAction);
    goMenu->addAction(homeAction);
    goMenu->addAction(m_browseAction);

    auto* editMenu = menuBar()->addMenu(i18nc("@title:menu", "&Edit"));
    editMenu->addAction(focusSearch);

    auto* settingsMenu = menuBar()->addMenu(i18nc("@title:menu", "&Settings"));
    settingsMenu->addAction(m_showMenubarAction);
    settingsMenu->addSeparator();
    settingsMenu->addAction(prefsAction);

    auto* helpMenu = menuBar()->addMenu(i18nc("@title:menu", "&Help"));
    helpMenu->addAction(aboutAction);
}

KHamburgerMenu* MainWindow::buildHamburgerMenu(QAction* quitAction,
    QAction* focusSearch,
    QAction* homeAction,
    QAction* prefsAction,
    QAction* aboutAction)
{
    // We deliberately do NOT call setMenuBar() on the hamburger: that would
    // mirror the classic menu bar's submenu structure (File / Edit / Settings
    // / Help), which is overkill for our handful of entries. Instead we build
    // a single flat menu grouped by horizontal separators.
    auto* hamburger = KStandardAction::hamburgerMenu(
        nullptr, nullptr, m_actions);
    hamburger->setShowMenuBarAction(m_showMenubarAction);

    connect(hamburger, &KHamburgerMenu::aboutToShowMenu, this,
        [this, hamburger, quitAction, focusSearch, homeAction,
            prefsAction, aboutAction] {
            auto* menu = new QMenu;
            menu->addAction(m_backAction);
            menu->addAction(homeAction);
            menu->addAction(m_browseAction);
            menu->addAction(focusSearch);
            menu->addSeparator();
            menu->addAction(m_showMenubarAction);
            menu->addAction(prefsAction);
            menu->addSeparator();
            menu->addAction(aboutAction);
            menu->addSeparator();
            menu->addAction(quitAction);
            hamburger->setMenu(menu);

            // Build once, on demand. KHamburgerMenu reuses the installed
            // menu for subsequent clicks.
            disconnect(hamburger, &KHamburgerMenu::aboutToShowMenu,
                this, nullptr);
        });

    return hamburger;
}

void MainWindow::buildToolbarActions(QAction* homeAction, KHamburgerMenu* hamburger)
{
    if (!m_toolbar) {
        return;
    }

    // Prepend [Back][Home][Browse] so they sit before the SearchBar
    // and are easy to reach with a single click (browser-style).
    QAction* firstAction = m_toolbar->actions().value(0);
    if (firstAction) {
        m_toolbar->insertAction(firstAction, m_backAction);
        m_toolbar->insertAction(firstAction, homeAction);
        m_toolbar->insertAction(firstAction, m_browseAction);
    } else {
        m_toolbar->addAction(m_backAction);
        m_toolbar->addAction(homeAction);
        m_toolbar->addAction(m_browseAction);
    }
    auto* spacer = new QWidget(m_toolbar);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    m_toolbar->addWidget(spacer);
    m_toolbar->addAction(hamburger);
}

void MainWindow::restoreMenubarVisibility()
{
    const bool showBar = m_settings.appearance().showMenuBar();
    m_showMenubarAction->setChecked(showBar);
    menuBar()->setVisible(showBar);
}

void MainWindow::onShowMenubarToggled(bool visible)
{
    menuBar()->setVisible(visible);
    m_settings.appearance().setShowMenuBar(visible);
}

void MainWindow::showAbout()
{
    auto* dlg = new KAboutApplicationDialog(KAboutData::applicationData(), this);
    dlg->setAttribute(Qt::WA_DeleteOnClose);
    dlg->open();
}

void MainWindow::onQueryRequested(const QString& text, api::MediaKind kind)
{
    m_search->runQuery(text, kind);
}

void MainWindow::onResultActivated(const QModelIndex& index)
{
    const auto* s = m_resultsModel->at(index.row());
    if (!s) {
        return;
    }

    // Clicking the already-loaded card is a no-op — avoids duplicate
    // network fetches and pane flicker.
    const bool panelOpen = m_nav
        && controllers::isDetailPage(m_nav->current());
    if (panelOpen) {
        if (s->kind == api::MediaKind::Movie
            && m_movieCtrl->current()
            && m_movieCtrl->current()->imdbId == s->imdbId) {
            return;
        }
        if (s->kind == api::MediaKind::Series
            && m_seriesCtrl->currentImdbId() == s->imdbId) {
            return;
        }
    }

    if (s->kind == api::MediaKind::Series) {
        m_seriesCtrl->openFromSummary(*s);
    } else {
        m_movieCtrl->openFromSummary(*s);
    }
}

void MainWindow::onEpisodeSelected(const api::Episode& ep)
{
    m_seriesCtrl->selectEpisode(ep);
}

void MainWindow::showDiscoverHome()
{
    // Cancel any open detail panel and close the search view so the
    // grid side of the splitter is fully given over to Discover.
    closeDetailPanel();
    if (m_nav) {
        m_nav->goTo(controllers::Page::DiscoverHome);
    }
    if (m_searchBar) {
        m_searchBar->clearFocus();
    }
    statusBar()->clearMessage();
    setWindowTitle(QStringLiteral("Kinema"));
}

void MainWindow::showBrowsePage()
{
    // Swap the results stack to Browse and close any detail pane so
    // the full content area is given over to the filter grid.
    closeDetailPanel();
    if (m_nav) {
        m_nav->goTo(controllers::Page::Browse);
    }
    if (m_browsePage) {
        // Fetch page 1 under the current filter state. refresh() is
        // idempotent and respects the not-configured CTA.
        m_browsePage->refresh();
    }
    if (m_searchBar) {
        m_searchBar->clearFocus();
    }
    statusBar()->clearMessage();
    setWindowTitle(i18nc("@title:window", "Kinema — Browse"));
}

void MainWindow::onDiscoverItemActivated(const api::DiscoverItem& item)
{
    if (item.kind == api::MediaKind::Movie) {
        m_movieCtrl->openFromDiscover(item);
    } else {
        m_seriesCtrl->openFromDiscover(item);
    }
}

void MainWindow::showSettings()
{
    if (!m_settingsDialog) {
        m_settingsDialog = new settings::SettingsDialog(
            m_http.get(), m_tokens.get(), m_settings, this);
        m_settingsDialog->setAttribute(Qt::WA_DeleteOnClose);
        connect(m_settingsDialog, &settings::SettingsDialog::tokenChanged,
            this, [this](const QString& token) {
                // Refresh TokenController's in-memory copy from the
                // keyring. The forwarded value is the just-saved
                // token; refreshing re-reads to keep the single
                // source-of-truth in TokenController.
                m_tokenCtrl->refreshRealDebrid();
                statusBar()->showMessage(
                    token.isEmpty()
                        ? i18nc("@info:status", "Real-Debrid token removed.")
                        : i18nc("@info:status", "Real-Debrid token saved."),
                    3000);
            });
        connect(m_settingsDialog, &settings::SettingsDialog::tmdbTokenChanged,
            this, [this](const QString&) {
                // Don't trust the forwarded value — the effective
                // token could be falling back to the compile-time
                // default. Re-resolve via TokenController.
                m_tokenCtrl->refreshTmdb();
                statusBar()->showMessage(
                    i18nc("@info:status", "TMDB token updated."), 3000);
            });
        connect(m_settingsDialog, &QObject::destroyed, this,
            [this] { m_settingsDialog = nullptr; });
    }
    m_settingsDialog->show();
    m_settingsDialog->raise();
    m_settingsDialog->activateWindow();
}

void MainWindow::onTorrentioOptionsChanged()
{
    if (m_refetchPending) {
        return;
    }
    m_refetchPending = true;
    QTimer::singleShot(0, this, [this] {
        m_refetchPending = false;
        if (!m_nav || !controllers::isDetailPage(m_nav->current())) {
            // No detail view visible — next open picks up the new
            // defaults. Nothing to refetch.
            return;
        }
        if (m_nav->current() == controllers::Page::SeriesStreams) {
            m_seriesCtrl->refetchCurrentEpisode();
            return;
        }
        if (m_nav->current() == controllers::Page::MovieDetail) {
            m_movieCtrl->refetchCurrent();
        }
    });
}

// ---- Window lifecycle -----------------------------------------------------

void MainWindow::showMainWindow()
{
    show();
    if (isMinimized()) {
        setWindowState(windowState() & ~Qt::WindowMinimized);
    }
    raise();
    activateWindow();
    m_tray->refreshMenu();
}

void MainWindow::hideMainWindow()
{
    hide();
    m_tray->refreshMenu();
}

void MainWindow::toggleMainWindow()
{
    if (isVisible() && !isMinimized()) {
        hideMainWindow();
    } else {
        showMainWindow();
    }
}

void MainWindow::quitApplication()
{
    m_reallyQuit = true;
#ifdef KINEMA_HAVE_LIBMPV
    // Take down the player window explicitly so its closeEvent can
    // persist geometry and stop playback before the app exits.
    if (m_playerWindow) {
        m_playerWindow->close();
    }
#endif
    qApp->quit();
}

void MainWindow::closeEvent(QCloseEvent* event)
{
    if (m_reallyQuit || !m_tray->available()
            || !m_settings.appearance().closeToTray()) {
        // Genuine exit path: accept the close and let Qt tear the
        // app down normally.
        event->accept();
        return;
    }

    hide();
    m_tray->refreshMenu();

    if (!m_hasShownTrayToast) {
        m_hasShownTrayToast = true;
        auto* n = new KNotification(
            QStringLiteral("trayMinimized"),
            KNotification::CloseOnTimeout);
        n->setTitle(i18nc("@title:window notification",
            "Kinema is still running"));
        n->setText(i18nc("@info notification",
            "Closing the main window hid Kinema to the system tray. "
            "Right-click the tray icon to show or quit."));
        n->setIconName(QStringLiteral("dev.tlmtech.kinema"));
        n->sendEvent();
    }

    event->ignore();
}

} // namespace kinema::ui
