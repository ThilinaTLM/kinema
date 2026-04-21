// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/MainWindow.h"

#include "api/CinemetaClient.h"
#include "api/TorrentioClient.h"
#include "config/Config.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/Magnet.h"
#include "core/PlayerLauncher.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"
#include "ui/DetailPane.h"
#include "ui/ImageLoader.h"
#include "ui/ResultCardDelegate.h"
#include "ui/ResultsModel.h"
#include "ui/SearchBar.h"
#include "ui/SeriesFocusView.h"
#include "ui/StateWidget.h"
#include "ui/settings/SettingsDialog.h"

#include <QEvent>

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
#include <QIcon>
#include <QKeySequence>
#include <QListView>
#include <QMenuBar>
#include <QRegularExpression>
#include <QSplitter>
#include <QStackedWidget>
#include <QStatusBar>
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

    if (!m_player->preferredPlayerAvailable()) {
        qCInfo(KINEMA) << "preferred media player not found on $PATH";
    }

    // RD state is driven by Config ("configured" bit) + the in-memory token.
    const bool hasRD = config::Config::instance().hasRealDebrid();
    m_detailPane->setRealDebridConfigured(hasRD);
    m_focusView->setRealDebridConfigured(hasRD);
    connect(&config::Config::instance(), &config::Config::realDebridChanged,
        this, [this](bool on) {
            m_detailPane->setRealDebridConfigured(on);
            m_focusView->setRealDebridConfigured(on);
        });

    // Load the token from the keyring in the background.
    if (hasRD) {
        auto t = loadRealDebridToken();
        Q_UNUSED(t);
    }

    // Refetch visible torrents whenever a Torrentio filter/sort default
    // changes. Multiple setters in one Apply click emit the signal
    // multiple times; debounce via a single-shot.
    connect(&config::Config::instance(),
        &config::Config::torrentioOptionsChanged,
        this, &MainWindow::onTorrentioOptionsChanged);
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

    m_resultsStack = new QStackedWidget(this);
    m_resultsStack->addWidget(m_resultsState);
    m_resultsStack->addWidget(m_resultsView);

    // ---- Detail pane (movies only) ----------------------------------------
    m_detailPane = new DetailPane(m_imageLoader, this);
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

    // ---- Browse view (results grid + DetailPane in a splitter) -----------
    auto* splitter = new QSplitter(Qt::Horizontal);
    splitter->addWidget(m_resultsStack);
    splitter->addWidget(m_detailPane);
    splitter->setStretchFactor(0, 1);
    splitter->setStretchFactor(1, 2);
    splitter->setChildrenCollapsible(false);

    m_browseView = new QWidget(this);
    auto* browseLayout = new QVBoxLayout(m_browseView);
    browseLayout->setContentsMargins(0, 0, 0, 0);
    browseLayout->addWidget(splitter);

    // ---- Series focus view -----------------------------------------------
    m_focusView = new SeriesFocusView(m_imageLoader, this);
    connect(m_focusView, &SeriesFocusView::backRequested,
        this, &MainWindow::onBackFromFocus);
    connect(m_focusView, &SeriesFocusView::episodeSelected,
        this, &MainWindow::onEpisodeSelected);
    connect(m_focusView, &SeriesFocusView::copyMagnetRequested,
        this, &MainWindow::onCopyMagnet);
    connect(m_focusView, &SeriesFocusView::openMagnetRequested,
        this, &MainWindow::onOpenMagnet);
    connect(m_focusView, &SeriesFocusView::copyDirectUrlRequested,
        this, &MainWindow::onCopyDirectUrl);
    connect(m_focusView, &SeriesFocusView::openDirectUrlRequested,
        this, &MainWindow::onOpenDirectUrl);
    connect(m_focusView, &SeriesFocusView::playRequested,
        this, &MainWindow::onPlayRequested);

    // ---- Outer stack ------------------------------------------------------
    m_viewStack = new QStackedWidget(this);
    m_viewStack->addWidget(m_browseView);
    m_viewStack->addWidget(m_focusView);
    m_viewStack->setCurrentWidget(m_browseView);
    setCentralWidget(m_viewStack);

    statusBar()->showMessage(i18nc("@info:status", "Ready"), 2000);
}

void MainWindow::buildActions()
{
    m_actions = new KActionCollection(this);

    // ---- Standard actions --------------------------------------------------
    auto* quitAction = KStandardAction::quit(
        qApp, &QApplication::quit, m_actions);
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

    // ---- Classic menu bar (hidden by default, toggled via Ctrl+M) ----------
    auto* fileMenu = menuBar()->addMenu(i18nc("@title:menu", "&File"));
    fileMenu->addAction(quitAction);

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
            [this, hamburger, focusSearch, prefsAction,
             aboutAction, quitAction] {
        auto* menu = new QMenu;
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
    m_resultsState->showLoading(i18nc("@info:status", "Searching…"));
    m_resultsStack->setCurrentWidget(m_resultsState);
    m_detailPane->showIdle();
    m_currentMovie.reset();
    m_currentEpisode.reset();
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

        // Auto-select and auto-load the first result's details.
        const auto firstIndex = m_resultsModel->index(0, 0);
        m_resultsView->setCurrentIndex(firstIndex);
        if (results.first().kind == api::MediaKind::Series) {
            auto t = loadSeriesDetail(results.first());
            Q_UNUSED(t);
        } else {
            auto t = loadMovieDetail(results.first());
            Q_UNUSED(t);
        }

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
    if (s->kind == api::MediaKind::Series) {
        auto t = loadSeriesDetail(*s);
        Q_UNUSED(t);
    } else {
        auto t = loadMovieDetail(*s);
        Q_UNUSED(t);
    }
}

void MainWindow::onEpisodeSelected(const api::Episode& ep)
{
    if (m_currentSeriesImdbId.isEmpty()) {
        return;
    }
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

    // Push into focus view and show loading state up front.
    m_focusView->showMetaLoading();
    m_viewStack->setCurrentWidget(m_focusView);

    try {
        auto sd = co_await m_cinemeta->seriesMeta(summary.imdbId);
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_focusView->setSeries(sd);

        // Update the window title for context.
        const auto& s = sd.meta.summary;
        setWindowTitle(s.year
                ? i18nc("@title:window", "Kinema \u2014 %1 (%2)",
                    s.title, QString::number(*s.year))
                : i18nc("@title:window", "Kinema \u2014 %1", s.title));

        // Give keyboard focus to the episode list for immediate arrow-key nav.
        m_focusView->focusEpisodeList();

        // Auto-select S1E1 (or the first non-special episode).
        const api::Episode* first = nullptr;
        for (const auto& e : sd.episodes) {
            if (e.season > 0) {
                first = &e;
                break;
            }
        }
        if (!first) {
            m_focusView->showTorrentsEmpty();
            co_return;
        }
        auto t = loadEpisodeStreams(*first, summary.imdbId);
        Q_UNUSED(t);

    } catch (const core::HttpError& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "series detail failed:" << e.message();
        m_focusView->showMetaError(e.message());
    } catch (const std::exception& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_focusView->showMetaError(QString::fromUtf8(e.what()));
    }
}

QCoro::Task<void> MainWindow::loadEpisodeStreams(api::Episode episode, QString imdbId)
{
    const auto myEpoch = ++m_episodeEpoch;
    m_currentEpisode = episode;
    m_focusView->showTorrentsLoading();

    const auto streamId = episode.streamId(imdbId);

    try {
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Series, streamId, currentConfig());
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_focusView->setTorrents(std::move(streams));
    } catch (const core::HttpError& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "episode streams failed:" << e.message();
        m_focusView->showTorrentsError(e.message());
    } catch (const std::exception& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_focusView->showTorrentsError(QString::fromUtf8(e.what()));
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

void MainWindow::onBackFromFocus()
{
    m_viewStack->setCurrentWidget(m_browseView);
    setWindowTitle(QStringLiteral("Kinema"));
    // Bump the episode epoch so any in-flight streams call bails out
    // without painting over the next selection.
    ++m_episodeEpoch;
    m_currentSeriesImdbId.clear();
    m_currentEpisode.reset();

    // Put focus back on the results grid so arrow keys work immediately.
    if (m_resultsView) {
        m_resultsView->setFocus();
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
        // Refetch whichever view is currently holding torrents.
        if (m_viewStack && m_viewStack->currentWidget() == m_focusView
            && m_currentEpisode && !m_currentSeriesImdbId.isEmpty()) {
            auto t = loadEpisodeStreams(*m_currentEpisode, m_currentSeriesImdbId);
            Q_UNUSED(t);
            return;
        }
        if (m_viewStack && m_viewStack->currentWidget() == m_browseView
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

} // namespace kinema::ui
