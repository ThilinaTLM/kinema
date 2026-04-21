// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/MainWindow.h"

#include "api/CinemetaClient.h"
#include "api/TmdbClient.h"
#include "api/TorrentioClient.h"
#include "config/Config.h"
#include "core/DateFormat.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/Magnet.h"
#include "core/PlayerLauncher.h"
#include "core/TmdbConfig.h"
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
#include <KStatusNotifierItem>

#include <KAboutApplicationDialog>
#include <KAboutData>
#include <KActionCollection>
#include <KConfigGroup>
#include <KHamburgerMenu>
#include <KIO/OpenUrlJob>
#include <KLocalizedString>
#include <KSharedConfig>
#include <KStandardAction>

#include <QAction>
#include <QApplication>
#include <QClipboard>
#include <QCloseEvent>
#include <QIcon>
#include <QKeySequence>
#include <QListView>
#include <QMenu>
#include <QMenuBar>
#include <QRegularExpression>
#include <QShortcut>
#include <QStackedWidget>
#include <QStatusBar>
#include <QSystemTrayIcon>
#include <QTimer>
#include <QToolBar>
#include <QVBoxLayout>

namespace kinema::ui {

namespace {
bool looksLikeImdbId(const QString& s)
{
    static const QRegularExpression re(QStringLiteral("^tt\\d{5,}$"));
    return re.match(s).hasMatch();
}
} // namespace

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent)
{
    setWindowTitle(QStringLiteral("Kinema"));
    setWindowIcon(QIcon::fromTheme(QStringLiteral("dev.tlmtech.kinema")));
    resize(1200, 780);

    // ---- Core services -----------------------------------------------------
    m_http = std::make_unique<core::HttpClient>(this);
    m_tokens = std::make_unique<core::TokenStore>(this);
    m_player = std::make_unique<core::PlayerLauncher>(this);
    m_cinemeta = new api::CinemetaClient(m_http.get(), this);
    m_torrentio = new api::TorrentioClient(m_http.get(), this);
    m_tmdb = new api::TmdbClient(m_http.get(), this);
    m_imageLoader = new ImageLoader(m_http.get(), this);

    buildLayout();
    buildActions();

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
#ifdef KINEMA_HAVE_LIBMPV
    connect(m_player.get(), &core::PlayerLauncher::embeddedRequested,
        this, &MainWindow::openEmbeddedPlayer);
#endif

    if (!m_player->preferredPlayerAvailable()) {
        qCInfo(KINEMA) << "preferred media player not found on $PATH";
    }

    // RD state is driven by Config ("configured" bit) + the in-memory token.
    const bool hasRD = config::Config::instance().hasRealDebrid();
    m_detailPane->setRealDebridConfigured(hasRD);
    m_seriesDetailPane->setRealDebridConfigured(hasRD);
    connect(&config::Config::instance(), &config::Config::realDebridChanged,
        this, [this](bool on) {
            m_detailPane->setRealDebridConfigured(on);
            m_seriesDetailPane->setRealDebridConfigured(on);
        });

    // Load the token from the keyring in the background.
    if (hasRD) {
        auto t = loadRealDebridToken();
        Q_UNUSED(t);
    }

    // Resolve the effective TMDB token and kick off the initial
    // Discover refresh. Fire-and-forget; the result populates
    // m_tmdbToken and either refreshes the rows or shows the
    // not-configured call-to-action.
    {
        auto t = loadTmdbToken();
        Q_UNUSED(t);
    }

    // Refetch visible torrents whenever a Torrentio filter/sort default
    // changes. Multiple setters in one Apply click emit the signal
    // multiple times; debounce via a single-shot.
    connect(&config::Config::instance(),
        &config::Config::torrentioOptionsChanged,
        this, &MainWindow::onTorrentioOptionsChanged);

    // System tray. No-op on desktops that don't expose a tray; the
    // close-to-tray path silently falls back to "close = quit" there.
    buildTray();
}

MainWindow::~MainWindow() = default;

void MainWindow::buildLayout()
{
    // ---- Search bar in a tool bar ------------------------------------------
    m_toolbar = addToolBar(i18nc("@title:window", "Search"));
    m_toolbar->setMovable(false);
    m_toolbar->setFloatable(false);
    m_toolbar->setContextMenuPolicy(Qt::PreventContextMenu);
    m_searchBar = new SearchBar(m_toolbar);
    m_searchBar->setMediaKind(config::Config::instance().searchKind());
    m_toolbar->addWidget(m_searchBar);
    connect(m_searchBar, &SearchBar::queryRequested,
        this, &MainWindow::onQueryRequested);
    connect(m_searchBar, &SearchBar::mediaKindChanged, this,
        [](api::MediaKind kind) {
            config::Config::instance().setSearchKind(kind);
        });

    // ---- Results view ------------------------------------------------------
    m_resultsModel = new ResultsModel(this);
    m_resultsView = new QListView(this);
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
    m_browsePage = new BrowsePage(m_tmdb, m_imageLoader, this);
    connect(m_browsePage, &BrowsePage::itemActivated,
        this, &MainWindow::onDiscoverItemActivated);
    connect(m_browsePage, &BrowsePage::settingsRequested,
        this, &MainWindow::showSettings);

    m_resultsStack = new QStackedWidget(this);
    m_resultsStack->addWidget(m_discoverPage); // idx 0 = home
    m_resultsStack->addWidget(m_resultsState); // idx 1 = search state
    m_resultsStack->addWidget(m_resultsView);  // idx 2 = search results
    m_resultsStack->addWidget(m_browsePage);   // idx 3 = browse

    // ---- Detail panes (movies + series) ----------------------------------
    m_detailPane = new DetailPane(m_imageLoader, m_tmdb, this);
    connect(m_detailPane, &DetailPane::similarActivated, this,
        [this](const api::DiscoverItem& item) {
            auto t = openFromDiscover(item);
            Q_UNUSED(t);
        });
    connect(m_detailPane, &DetailPane::copyMagnetRequested,
        this, &MainWindow::onCopyMagnet);
    connect(m_detailPane, &DetailPane::openMagnetRequested,
        this, &MainWindow::onOpenMagnet);
    connect(m_detailPane, &DetailPane::copyDirectUrlRequested,
        this, &MainWindow::onCopyDirectUrl);
    connect(m_detailPane, &DetailPane::openDirectUrlRequested,
        this, &MainWindow::onOpenDirectUrl);
    connect(m_detailPane, &DetailPane::playRequested,
        this, &MainWindow::onPlayRequested);

    m_seriesDetailPane = new SeriesDetailPane(m_imageLoader, m_tmdb, this);
    connect(m_seriesDetailPane, &SeriesDetailPane::similarActivated, this,
        [this](const api::DiscoverItem& item) {
            auto t = openFromDiscover(item);
            Q_UNUSED(t);
        });
    connect(m_seriesDetailPane, &SeriesDetailPane::episodeSelected,
        this, &MainWindow::onEpisodeSelected);
    connect(m_seriesDetailPane, &SeriesDetailPane::backToEpisodesRequested,
        this, &MainWindow::onBackToEpisodes);
    connect(m_seriesDetailPane, &SeriesDetailPane::copyMagnetRequested,
        this, &MainWindow::onCopyMagnet);
    connect(m_seriesDetailPane, &SeriesDetailPane::openMagnetRequested,
        this, &MainWindow::onOpenMagnet);
    connect(m_seriesDetailPane, &SeriesDetailPane::copyDirectUrlRequested,
        this, &MainWindow::onCopyDirectUrl);
    connect(m_seriesDetailPane, &SeriesDetailPane::openDirectUrlRequested,
        this, &MainWindow::onOpenDirectUrl);
    connect(m_seriesDetailPane, &SeriesDetailPane::playRequested,
        this, &MainWindow::onPlayRequested);

    // ---- Center stack: results (page 0) + detail panes (1, 2) ------------
    m_centerStack = new QStackedWidget(this);
    m_centerStack->addWidget(m_resultsStack);       // idx 0 = results
    m_centerStack->addWidget(m_detailPane);         // idx 1 = movie
    m_centerStack->addWidget(m_seriesDetailPane);   // idx 2 = series
    m_centerStack->setCurrentWidget(m_resultsStack);

    setCentralWidget(m_centerStack);

    // Refresh the toolbar Back action whenever the navigation surface
    // changes (detail opens/closes, or the results stack flips between
    // Discover / search state / search results).
    connect(m_centerStack, &QStackedWidget::currentChanged,
        this, [this] { updateBackActionEnabled(); });
    connect(m_resultsStack, &QStackedWidget::currentChanged,
        this, [this] { updateBackActionEnabled(); });

    statusBar()->showMessage(i18nc("@info:status", "Ready"), 2000);
}

void MainWindow::showMovieDetail()
{
    if (m_centerStack) {
        m_centerStack->setCurrentWidget(m_detailPane);
    }
}

void MainWindow::showSeriesDetail()
{
    if (m_centerStack) {
        m_centerStack->setCurrentWidget(m_seriesDetailPane);
    }
}

void MainWindow::closeDetailPanel()
{
    if (!m_centerStack
        || m_centerStack->currentWidget() == m_resultsStack) {
        return;
    }

    // Cancel any in-flight work so stale fetches don't repopulate the
    // pane after it's been dismissed.
    ++m_detailEpoch;
    ++m_episodeEpoch;
    m_currentMovie.reset();
    m_currentEpisode.reset();
    m_currentSeriesImdbId.clear();

    // Reset both panes to their idle states so re-opening a different
    // kind later doesn't momentarily flash stale content.
    m_detailPane->showIdle();
    m_seriesDetailPane->showMetaLoading();

    m_centerStack->setCurrentWidget(m_resultsStack);

    // Clear selection so clicking the same card re-opens cleanly.
    if (auto* sm = m_resultsView->selectionModel()) {
        sm->clearSelection();
        sm->clearCurrentIndex();
    }
    if (m_resultsStack->currentWidget() == m_resultsView) {
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
        m_playerWindow = new player::PlayerWindow(this);

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
            this, [this](bool) { updateTrayMenu(); });
    }

    statusBar()->showMessage(
        i18nc("@info:status", "Loading “%1”…", title), 3000);

    m_playerWindow->play(url, title);
}

#endif // KINEMA_HAVE_LIBMPV

void MainWindow::onBackToEpisodes()
{
    // User backed out of the streams page (via back button or Esc).
    // Drop any in-flight stream task so its late response can't
    // repopulate the (now-hidden) torrents table the next time the
    // streams page is shown.
    ++m_episodeEpoch;
    m_currentEpisode.reset();
}

void MainWindow::onBackRequested()
{
    // The embedded player lives in its own top-level window and owns
    // its own Back / Esc handling, so MainWindow's back action only
    // walks the main-window nav stack (streams → episodes → detail
    // → results → Discover).

    // 1. Series streams page → episodes list (stays in the pane).
    if (m_centerStack
        && m_centerStack->currentWidget() == m_seriesDetailPane
        && m_seriesDetailPane->tryPopStreamsPage()) {
        onBackToEpisodes();
        updateBackActionEnabled();
        return;
    }

    // 2. Any detail pane → results stack (whatever sub-page was
    //    showing before the pane opened).
    if (m_centerStack
        && m_centerStack->currentWidget() != m_resultsStack) {
        closeDetailPanel();
        // currentChanged fires → updateBackActionEnabled runs.
        return;
    }

    // 3. Search state / search results → Discover home.
    if (m_resultsStack
        && m_resultsStack->currentWidget() != m_discoverPage) {
        showDiscoverHome();
        return;
    }

    // 4. Already at Discover home — button should have been disabled.
}

void MainWindow::updateBackActionEnabled()
{
    if (!m_backAction) {
        return;
    }
    const bool atDiscoverHome = m_centerStack
        && m_centerStack->currentWidget() == m_resultsStack
        && m_resultsStack
        && m_resultsStack->currentWidget() == m_discoverPage;
    m_backAction->setEnabled(!atDiscoverHome);
}

void MainWindow::buildActions()
{
    m_actions = new KActionCollection(this);

    // ---- Standard actions --------------------------------------------------
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

    // ---- Custom actions ----------------------------------------------------
    auto* focusSearch = new QAction(
        QIcon::fromTheme(QStringLiteral("search")),
        i18nc("@action:inmenu", "&Focus search"), this);
    focusSearch->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_K));
    connect(focusSearch, &QAction::triggered, this, [this] {
        m_searchBar->setFocus();
    });
    m_actions->addAction(QStringLiteral("focus_search"), focusSearch);

    // Home — swap back to the Discover page from any search result.
    auto* homeAction = KStandardAction::home(
        this, &MainWindow::showDiscoverHome, m_actions);
    homeAction->setToolTip(i18nc("@info:tooltip", "Back to Discover"));

    // Browse — third top-level surface (after Discover home and
    // free-text search). Drives the TMDB /discover filter grid.
    // Icon: a grid-of-squares (Breeze: `view-list-icons`) — visually
    // matches the wrap-grid of posters the user lands on. We
    // deliberately avoid `view-filter` here since that's the icon
    // used by the Genres button *inside* the filter bar; reusing it
    // on the toolbar would conflate "navigate to Browse" with
    // "apply a filter".
    m_browseAction = new QAction(
        QIcon::fromTheme(QStringLiteral("view-list-icons")),
        i18nc("@action:inmenu", "&Browse"), this);
    m_browseAction->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_B));
    m_browseAction->setToolTip(i18nc("@info:tooltip",
        "Browse movies and TV with filters (Ctrl+B)"));
    connect(m_browseAction, &QAction::triggered,
        this, &MainWindow::showBrowsePage);
    m_actions->addAction(QStringLiteral("browse_page"), m_browseAction);

    // Back — browser-style "one level up" walker, sibling to Home.
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
        if (m_centerStack
            && m_centerStack->currentWidget() == m_seriesDetailPane
            && m_seriesDetailPane->tryPopStreamsPage()) {
            onBackToEpisodes();
            return;
        }
        if (m_centerStack
            && m_centerStack->currentWidget() != m_resultsStack) {
            closeDetailPanel();
        }
    });

    // ---- Classic menu bar (hidden by default, toggled via Ctrl+M) ----------
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

    // ---- Hamburger button on the toolbar -----------------------------------
    //
    // We deliberately do NOT call setMenuBar() on the hamburger: that would
    // mirror the classic menu bar's submenu structure (File / Edit / Settings
    // / Help), which is overkill for our handful of entries. Instead we build
    // a single flat menu grouped by horizontal separators.
    auto* hamburger = KStandardAction::hamburgerMenu(
        nullptr, nullptr, m_actions);
    hamburger->setShowMenuBarAction(m_showMenubarAction);
    // Deliberately NOT calling setMenuBar(): that would auto-append the
    // menu bar's Help menu and a "More" submenu advertising menu-bar-only
    // actions. We want a fully flat custom menu instead.

    connect(hamburger, &KHamburgerMenu::aboutToShowMenu, this,
            [this, hamburger, focusSearch, homeAction, prefsAction,
             aboutAction, quitAction] {
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

    if (m_toolbar) {
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

    // Bind all actions (and their shortcuts) to the main window so keys
    // like Ctrl+M / Ctrl+Q / Ctrl+K fire regardless of menu-bar state.
    m_actions->addAssociatedWidget(this);

    // ---- Restore persisted menu-bar visibility -----------------------------
    const auto group = KSharedConfig::openConfig()->group(
        QStringLiteral("MainWindow"));
    const bool showBar = group.readEntry("ShowMenuBar", false);
    m_showMenubarAction->setChecked(showBar);
    menuBar()->setVisible(showBar);

    // Cold start lands on Discover home, so Back has nowhere to go.
    updateBackActionEnabled();
}

void MainWindow::onShowMenubarToggled(bool visible)
{
    menuBar()->setVisible(visible);
    auto group = KSharedConfig::openConfig()->group(
        QStringLiteral("MainWindow"));
    group.writeEntry("ShowMenuBar", visible);
    group.sync();
}

void MainWindow::showAbout()
{
    auto* dlg = new KAboutApplicationDialog(KAboutData::applicationData(), this);
    dlg->setAttribute(Qt::WA_DeleteOnClose);
    dlg->open();
}

void MainWindow::onQueryRequested(const QString& text, api::MediaKind kind)
{
    // Fire-and-forget: each invocation is its own coroutine. Stale ones
    // are filtered out inside runSearch via the epoch counter.
    auto t = runSearch(text, kind);
    Q_UNUSED(t);
}

QCoro::Task<void> MainWindow::runSearch(QString text, api::MediaKind kind)
{
    const auto myEpoch = ++m_queryEpoch;

    if (m_resultsDelegate) {
        m_resultsDelegate->resetRequestTracking();
    }
    // Close any open panel so the user lands in a grid-first view on
    // every new search. closeDetailPanel() is idempotent when nothing
    // is open.
    closeDetailPanel();
    m_resultsState->showLoading(i18nc("@info:status", "Searching…"));
    m_resultsStack->setCurrentWidget(m_resultsState);
    statusBar()->showMessage(i18nc("@info:status", "Searching for \"%1\"…", text));

    try {
        QList<api::MetaSummary> results;
        if (looksLikeImdbId(text)) {
            // IMDB shortcut — synthesise a one-item list from the meta fetch.
            auto detail = co_await m_cinemeta->meta(kind, text);
            if (myEpoch != m_queryEpoch) {
                co_return;
            }
            results.append(detail.summary);
        } else {
            results = co_await m_cinemeta->search(kind, text);
            if (myEpoch != m_queryEpoch) {
                co_return;
            }
        }

        if (results.isEmpty()) {
            m_resultsModel->reset({});
            m_resultsState->showIdle(
                i18nc("@label", "No matches"),
                i18nc("@info", "Nothing found for \"%1\".", text));
            m_resultsStack->setCurrentWidget(m_resultsState);
            statusBar()->showMessage(i18nc("@info:status", "No matches"), 4000);
            co_return;
        }

        m_resultsModel->reset(results);
        m_resultsStack->setCurrentWidget(m_resultsView);
        statusBar()->showMessage(
            i18ncp("@info:status", "%1 result", "%1 results", results.size()),
            3000);
        // No auto-select: the panel stays closed until the user picks
        // a card, so the grid can take the full content area.
        m_resultsView->setFocus();

    } catch (const core::HttpError& e) {
        if (myEpoch != m_queryEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "search failed:" << e.message();
        m_resultsState->showError(e.message(), /*retryable=*/false);
        m_resultsStack->setCurrentWidget(m_resultsState);
        statusBar()->showMessage(i18nc("@info:status", "Search failed"), 4000);
    } catch (const std::exception& e) {
        if (myEpoch != m_queryEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "search failed (unknown):" << e.what();
        m_resultsState->showError(QString::fromUtf8(e.what()), /*retryable=*/false);
        m_resultsStack->setCurrentWidget(m_resultsState);
    }
}

void MainWindow::onResultActivated(const QModelIndex& index)
{
    const auto* s = m_resultsModel->at(index.row());
    if (!s) {
        return;
    }

    // Clicking the already-loaded card is a no-op — avoids duplicate
    // network fetches and pane flicker. "panelOpen" here means the
    // center stack is currently showing a detail pane rather than the
    // results stack.
    const bool panelOpen = m_centerStack
        && m_centerStack->currentWidget() != m_resultsStack;
    if (panelOpen) {
        if (s->kind == api::MediaKind::Movie && m_currentMovie
            && m_currentMovie->imdbId == s->imdbId) {
            return;
        }
        if (s->kind == api::MediaKind::Series
            && m_currentSeriesImdbId == s->imdbId) {
            return;
        }
    }

    if (s->kind == api::MediaKind::Series) {
        showSeriesDetail();
        auto t = loadSeriesDetail(*s);
        Q_UNUSED(t);
    } else {
        showMovieDetail();
        auto t = loadMovieDetail(*s);
        Q_UNUSED(t);
    }
}

void MainWindow::onEpisodeSelected(const api::Episode& ep)
{
    if (m_currentSeriesImdbId.isEmpty()) {
        return;
    }
    // The pane has already flipped to the streams page in response to
    // the user's click (see SeriesDetailPane's episodeActivated
    // handler). Kick off the stream fetch.
    auto t = loadEpisodeStreams(ep, m_currentSeriesImdbId);
    Q_UNUSED(t);
}

QCoro::Task<void> MainWindow::loadMovieDetail(api::MetaSummary summary)
{
    const auto myEpoch = ++m_detailEpoch;
    m_currentSeriesImdbId.clear();
    m_currentEpisode.reset();
    m_currentMovie = summary;
    m_detailPane->showMetaLoading();
    m_detailPane->showTorrentsLoading();

    try {
        auto detail = co_await m_cinemeta->meta(api::MediaKind::Movie, summary.imdbId);
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_detailPane->setMeta(detail);
        m_detailPane->setSimilarContext(
            api::MediaKind::Movie, summary.imdbId);

        // Short-circuit Torrentio for unreleased movies. Streams won't
        // exist before theatrical/streaming release, and the empty
        // result would read as an error. Show a dedicated "not yet
        // released" state instead, keyed on the actual release date.
        if (core::isFutureRelease(detail.summary.released)) {
            m_detailPane->showTorrentsUnreleased(*detail.summary.released);
            co_return;
        }

        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Movie, summary.imdbId, currentConfig());
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_detailPane->setTorrents(std::move(streams));

    } catch (const core::HttpError& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "detail/streams failed:" << e.message();
        m_detailPane->showTorrentsError(e.message());
    } catch (const std::exception& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_detailPane->showTorrentsError(QString::fromUtf8(e.what()));
    }
}

QCoro::Task<void> MainWindow::loadSeriesDetail(api::MetaSummary summary)
{
    const auto myEpoch = ++m_detailEpoch;
    m_currentSeriesImdbId = summary.imdbId;
    m_currentMovie.reset();
    m_currentEpisode.reset();

    // Show loading state in the series side of the detail stack.
    m_seriesDetailPane->showMetaLoading();

    try {
        auto sd = co_await m_cinemeta->seriesMeta(summary.imdbId);
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_seriesDetailPane->setSeries(sd);
        m_seriesDetailPane->setSimilarContext(summary.imdbId);

        // Land on the episode list — no auto-fetch. The user picks an
        // episode explicitly; picking one flips the right column to
        // the streams page and fires onEpisodeSelected.
        m_seriesDetailPane->focusEpisodeList();

    } catch (const core::HttpError& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "series detail failed:" << e.message();
        m_seriesDetailPane->showMetaError(e.message());
    } catch (const std::exception& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_seriesDetailPane->showMetaError(QString::fromUtf8(e.what()));
    }
}

QCoro::Task<void> MainWindow::loadEpisodeStreams(api::Episode episode, QString imdbId)
{
    const auto myEpoch = ++m_episodeEpoch;
    m_currentEpisode = episode;

    // Same short-circuit as movies: unaired episodes produce no useful
    // Torrentio result; surface the air date directly instead.
    if (core::isFutureRelease(episode.released)) {
        m_seriesDetailPane->showTorrentsUnreleased(*episode.released);
        co_return;
    }

    m_seriesDetailPane->showTorrentsLoading();

    const auto streamId = episode.streamId(imdbId);

    try {
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Series, streamId, currentConfig());
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_seriesDetailPane->setTorrents(std::move(streams));
    } catch (const core::HttpError& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "episode streams failed:" << e.message();
        m_seriesDetailPane->showTorrentsError(e.message());
    } catch (const std::exception& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_seriesDetailPane->showTorrentsError(QString::fromUtf8(e.what()));
    }
}

core::torrentio::ConfigOptions MainWindow::currentConfig() const
{
    auto opts = config::Config::instance().defaultTorrentioOptions();
    opts.realDebridToken = m_rdToken;
    return opts;
}

QCoro::Task<void> MainWindow::loadRealDebridToken()
{
    try {
        m_rdToken = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        qCDebug(KINEMA) << "RD token loaded from keyring (empty?"
                        << m_rdToken.isEmpty() << ")";
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "RD token read failed:" << e.what();
        m_rdToken.clear();
    }
}

QCoro::Task<void> MainWindow::loadTmdbToken()
{
    // User override wins if present; otherwise the compile-time
    // default; otherwise empty (Discover shows not-configured state).
    QString user;
    try {
        user = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "TMDB token read failed:" << e.what();
    }
    if (!user.isEmpty()) {
        m_tmdbToken = std::move(user);
    } else {
        const auto* def = core::kTmdbCompiledDefaultToken;
        m_tmdbToken = (def && def[0] != '\0')
            ? QString::fromLatin1(def)
            : QString {};
    }

    if (m_tmdb) {
        m_tmdb->setToken(m_tmdbToken);
    }
    if (m_discoverPage) {
        if (m_tmdbToken.isEmpty()) {
            m_discoverPage->showNotConfigured();
        } else {
            m_discoverPage->refresh();
        }
    }
    if (m_browsePage) {
        // Browse is lazy — don't refetch until the user visits it.
        // But the CTA needs to reflect the current token state so the
        // first visit lands on the right page.
        if (m_tmdbToken.isEmpty()) {
            m_browsePage->showNotConfigured();
        } else if (m_resultsStack
            && m_resultsStack->currentWidget() == m_browsePage) {
            // If the user is already on Browse when the token settles,
            // refetch immediately so they see results, not loading.
            m_browsePage->refresh();
        }
    }
}

void MainWindow::showDiscoverHome()
{
    // Cancel any open detail panel and close the search view so the
    // grid side of the splitter is fully given over to Discover.
    closeDetailPanel();
    if (m_resultsStack && m_discoverPage) {
        m_resultsStack->setCurrentWidget(m_discoverPage);
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
    if (m_resultsStack && m_browsePage) {
        m_resultsStack->setCurrentWidget(m_browsePage);
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
    auto t = openFromDiscover(item);
    Q_UNUSED(t);
}

QCoro::Task<void> MainWindow::openFromDiscover(api::DiscoverItem item)
{
    // Resolve TMDB id → IMDB id, then hand off to the existing detail
    // pipeline. Done on the click (not eagerly at list time) because
    // list endpoints don't include external_ids and most cards will
    // never be clicked.
    statusBar()->showMessage(
        i18nc("@info:status", "Looking up “%1”…", item.title));

    QString imdbId;
    try {
        imdbId = item.kind == api::MediaKind::Movie
            ? co_await m_tmdb->imdbIdForTmdbMovie(item.tmdbId)
            : co_await m_tmdb->imdbIdForTmdbSeries(item.tmdbId);
    } catch (const core::HttpError& e) {
        // 404 from /movie/{id} or /tv/{id}/external_ids means TMDB has no
        // record for this (kind, id) pair — usually a stale/ghost entry
        // in a list endpoint. Treat it like "no IMDB id" rather than a
        // hard error: no red log, soft status-bar message. Log the item
        // at warning level so the offending list/row can be identified.
        const bool notFound
            = e.kind() == core::HttpError::Kind::HttpStatus
            && e.httpStatus() == 404;
        qCWarning(KINEMA).nospace()
            << "TMDB external_ids lookup failed: "
            << (item.kind == api::MediaKind::Movie ? "movie" : "tv")
            << "/" << item.tmdbId << " (\"" << item.title << "\") — "
            << e.httpStatus() << " " << e.message();
        if (notFound) {
            statusBar()->showMessage(
                i18nc("@info:status",
                    "“%1” isn't reachable on TMDB — can't fetch streams.",
                    item.title),
                6000);
        } else {
            statusBar()->showMessage(
                i18nc("@info:status", "Could not open “%1”: %2",
                    item.title, e.message()),
                6000);
        }
        co_return;
    } catch (const std::exception& e) {
        statusBar()->showMessage(QString::fromUtf8(e.what()), 6000);
        co_return;
    }

    if (imdbId.isEmpty()) {
        qCWarning(KINEMA).nospace()
            << "TMDB has no IMDB id for "
            << (item.kind == api::MediaKind::Movie ? "movie" : "tv")
            << "/" << item.tmdbId << " (\"" << item.title << "\")";
        statusBar()->showMessage(
            i18nc("@info:status",
                "“%1” has no IMDB id on TMDB — can't fetch streams.",
                item.title),
            6000);
        co_return;
    }

    // Build a MetaSummary mirroring what search would have produced,
    // so the existing flow (which expects one) runs unchanged.
    api::MetaSummary s;
    s.imdbId = imdbId;
    s.kind = item.kind;
    s.title = item.title;
    s.year = item.year;
    s.poster = item.poster;
    s.description = item.overview;
    s.imdbRating = item.voteAverage;

    // Closing the detail view now returns to whichever results-stack
    // page is currently shown (Discover, search state, or search grid),
    // so there's no need to synthesise a one-card grid as a fallback.
    if (s.kind == api::MediaKind::Movie) {
        showMovieDetail();
        co_await loadMovieDetail(s);
    } else {
        showSeriesDetail();
        co_await loadSeriesDetail(s);
    }
}

void MainWindow::showSettings()
{
    if (!m_settingsDialog) {
        m_settingsDialog = new settings::SettingsDialog(
            m_http.get(), m_tokens.get(), this);
        m_settingsDialog->setAttribute(Qt::WA_DeleteOnClose);
        connect(m_settingsDialog, &settings::SettingsDialog::tokenChanged,
            this, [this](const QString& token) {
                m_rdToken = token;
                statusBar()->showMessage(
                    token.isEmpty()
                        ? i18nc("@info:status", "Real-Debrid token removed.")
                        : i18nc("@info:status", "Real-Debrid token saved."),
                    3000);
            });
        connect(m_settingsDialog, &settings::SettingsDialog::tmdbTokenChanged,
            this, [this](const QString&) {
                // Don't trust the forwarded value directly — the
                // effective token could be falling back to the
                // compile-time default. Re-resolve via the same path
                // we use at startup.
                auto t = loadTmdbToken();
                Q_UNUSED(t);
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
        if (!m_centerStack
            || m_centerStack->currentWidget() == m_resultsStack) {
            // No detail view visible — next open picks up the new
            // defaults. Nothing to refetch.
            return;
        }
        if (m_centerStack->currentWidget() == m_seriesDetailPane
            && m_currentEpisode && !m_currentSeriesImdbId.isEmpty()) {
            // Series detail: only refetch when the user is actually
            // looking at the streams page (i.e. m_currentEpisode is
            // set). On the episode list there are no visible streams.
            auto t = loadEpisodeStreams(
                *m_currentEpisode, m_currentSeriesImdbId);
            Q_UNUSED(t);
            return;
        }
        if (m_centerStack->currentWidget() == m_detailPane
            && m_currentMovie) {
            auto t = loadMovieDetail(*m_currentMovie);
            Q_UNUSED(t);
        }
    });
}

void MainWindow::onCopyMagnet(const api::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    const auto magnet = core::magnet::build(stream.infoHash, stream.releaseName);
    QGuiApplication::clipboard()->setText(magnet);
    statusBar()->showMessage(
        i18nc("@info:status", "Magnet link copied to clipboard"), 3000);
}

void MainWindow::onOpenMagnet(const api::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    const auto magnet = core::magnet::build(stream.infoHash, stream.releaseName);
    auto* job = new KIO::OpenUrlJob(QUrl(magnet), this);
    job->setRunExecutables(false);
    connect(job, &KJob::result, this, [this, job] {
        if (job->error()) {
            statusBar()->showMessage(
                i18nc("@info:status", "Could not open magnet: %1", job->errorString()),
                6000);
            qCWarning(KINEMA) << "OpenUrlJob failed:" << job->errorString();
        } else {
            statusBar()->showMessage(
                i18nc("@info:status", "Magnet sent to default handler"), 3000);
        }
    });
    job->start();
}

void MainWindow::onCopyDirectUrl(const api::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(stream.directUrl.toString());
    statusBar()->showMessage(
        i18nc("@info:status", "Direct URL copied to clipboard"), 3000);
}

void MainWindow::onPlayRequested(const api::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        statusBar()->showMessage(
            i18nc("@info:status",
                "This release has no direct URL \u2014 use Real-Debrid for one-click play."),
            5000);
        return;
    }
    m_player->play(stream.directUrl,
        stream.releaseName.isEmpty() ? stream.qualityLabel : stream.releaseName);
}

void MainWindow::onOpenDirectUrl(const api::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    auto* job = new KIO::OpenUrlJob(stream.directUrl, this);
    job->setRunExecutables(false);
    connect(job, &KJob::result, this, [this, job] {
        if (job->error()) {
            statusBar()->showMessage(
                i18nc("@info:status", "Could not open URL: %1", job->errorString()),
                6000);
            qCWarning(KINEMA) << "OpenUrlJob (direct) failed:" << job->errorString();
        } else {
            statusBar()->showMessage(
                i18nc("@info:status", "Opening stream\u2026"), 3000);
        }
    });
    job->start();
}

// ---- Tray + app lifecycle -------------------------------------------------

void MainWindow::buildTray()
{
    // Respect desktops that don't expose any tray at all (minimal
    // window managers, GNOME without extensions). KStatusNotifierItem
    // falls back from SNI to legacy QSystemTrayIcon automatically
    // when only the legacy mechanism is available, so this check
    // covers the genuinely-absent case.
    m_trayAvailable = QSystemTrayIcon::isSystemTrayAvailable();
    if (!m_trayAvailable) {
        qCInfo(KINEMA) << "no system tray host available;"
                       << "close-to-tray will be inert";
        return;
    }

    m_tray = new KStatusNotifierItem(
        QStringLiteral("kinema"), this);

    // KStatusNotifierItem auto-associates the main window when its
    // parent is a QWidget — on activation (tray click, DBus
    // Activate, etc.) it auto-toggles that window's visibility. We
    // want *our* handlers to be the single source of truth for
    // show/hide so that the behaviour matches the user's
    // close-to-tray preference and doesn't race with the player
    // window. Disable the association and the default "standard
    // actions" menu (Minimize/Restore/Quit) KSNI would otherwise
    // inject alongside the entries we add below.
    m_tray->setAssociatedWindow(nullptr);
    m_tray->setStandardActionsEnabled(false);

    m_tray->setCategory(KStatusNotifierItem::ApplicationStatus);
    m_tray->setStatus(KStatusNotifierItem::Active);
    m_tray->setTitle(QStringLiteral("Kinema"));
    m_tray->setIconByName(QStringLiteral("dev.tlmtech.kinema"));
    m_tray->setToolTipIconByName(QStringLiteral("dev.tlmtech.kinema"));
    m_tray->setToolTipTitle(QStringLiteral("Kinema"));
    m_tray->setToolTipSubTitle(
        i18nc("@info:tooltip tray icon",
            "Cinema in motion — click to show or hide Kinema."));

    // Primary activation (left-click) toggles the main window. The
    // bool argument is whether the request came from the primary
    // action; we treat secondary activation the same way since the
    // context menu already offers everything else.
    connect(m_tray, &KStatusNotifierItem::activateRequested,
        this, [this](bool /*active*/, const QPoint& /*pos*/) {
            if (isVisible() && !isMinimized()) {
                hideMainWindow();
            } else {
                showMainWindow();
            }
        });

    // Context menu entries. KStatusNotifierItem::contextMenu() auto-
    // creates a QMenu we can populate and re-populate.
    auto* menu = m_tray->contextMenu();

    m_trayToggleAction = new QAction(this);
    connect(m_trayToggleAction, &QAction::triggered, this, [this] {
        if (isVisible() && !isMinimized()) {
            hideMainWindow();
        } else {
            showMainWindow();
        }
    });
    menu->addAction(m_trayToggleAction);

#ifdef KINEMA_HAVE_LIBMPV
    m_trayShowPlayerAction = new QAction(
        QIcon::fromTheme(QStringLiteral("media-playback-start")),
        i18nc("@action:inmenu tray", "Show Player"), this);
    connect(m_trayShowPlayerAction, &QAction::triggered, this, [this] {
        if (m_playerWindow) {
            m_playerWindow->show();
            m_playerWindow->raise();
            m_playerWindow->activateWindow();
            updateTrayMenu();
        }
    });
    menu->addAction(m_trayShowPlayerAction);
#endif

    menu->addSeparator();

    auto* trayQuit = new QAction(
        QIcon::fromTheme(QStringLiteral("application-exit")),
        i18nc("@action:inmenu tray", "Quit Kinema"), this);
    connect(trayQuit, &QAction::triggered,
        this, &MainWindow::quitApplication);
    menu->addAction(trayQuit);

    updateTrayMenu();
}

void MainWindow::updateTrayMenu()
{
    if (!m_trayAvailable) {
        return;
    }
    if (m_trayToggleAction) {
        const bool shown = isVisible() && !isMinimized();
        if (shown) {
            m_trayToggleAction->setText(
                i18nc("@action:inmenu tray", "Hide Kinema"));
            m_trayToggleAction->setIcon(
                QIcon::fromTheme(QStringLiteral("window-close")));
        } else {
            m_trayToggleAction->setText(
                i18nc("@action:inmenu tray", "Show Kinema"));
            m_trayToggleAction->setIcon(
                QIcon::fromTheme(QStringLiteral("window")));
        }
    }
#ifdef KINEMA_HAVE_LIBMPV
    if (m_trayShowPlayerAction) {
        // Only meaningful when we have a player that was ever used
        // but is currently hidden — otherwise the entry would either
        // duplicate a visible window or point at nothing.
        const bool canShow = m_playerWindow
            && m_playerWindow->hasEverLoaded()
            && !m_playerWindow->isVisible();
        m_trayShowPlayerAction->setVisible(canShow);
    }
#endif
}

void MainWindow::showMainWindow()
{
    show();
    if (isMinimized()) {
        setWindowState(windowState() & ~Qt::WindowMinimized);
    }
    raise();
    activateWindow();
    updateTrayMenu();
}

void MainWindow::hideMainWindow()
{
    hide();
    updateTrayMenu();
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
    if (m_reallyQuit || !m_trayAvailable
            || !config::Config::instance().closeToTray()) {
        // Genuine exit path: accept the close and let Qt tear the
        // app down normally.
        event->accept();
        return;
    }

    hide();
    updateTrayMenu();

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
