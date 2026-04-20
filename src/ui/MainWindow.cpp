// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/MainWindow.h"

#include "api/CinemetaClient.h"
#include "api/TorrentioClient.h"
#include "config/Config.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/Magnet.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"
#include "ui/DetailPane.h"
#include "ui/ImageLoader.h"
#include "ui/RealDebridDialog.h"
#include "ui/ResultCardDelegate.h"
#include "ui/ResultsModel.h"
#include "ui/SearchBar.h"
#include "ui/StateWidget.h"

#include <KAboutApplicationDialog>
#include <KAboutData>
#include <KIO/OpenUrlJob>
#include <KLocalizedString>

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
#include <QToolBar>

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
    m_cinemeta = new api::CinemetaClient(m_http.get(), this);
    m_torrentio = new api::TorrentioClient(m_http.get(), this);
    m_imageLoader = new ImageLoader(m_http.get(), this);

    buildLayout();
    buildActions();

    // RD state is driven by Config ("configured" bit) + the in-memory token.
    const bool hasRD = config::Config::instance().hasRealDebrid();
    m_detailPane->setRealDebridConfigured(hasRD);
    connect(&config::Config::instance(), &config::Config::realDebridChanged,
        this, [this](bool on) {
            m_detailPane->setRealDebridConfigured(on);
        });

    // Load the token from the keyring in the background.
    if (hasRD) {
        auto t = loadRealDebridToken();
        Q_UNUSED(t);
    }
}

MainWindow::~MainWindow() = default;

void MainWindow::buildLayout()
{
    // ---- Search bar in a tool bar ------------------------------------------
    auto* toolbar = addToolBar(i18nc("@title:window", "Search"));
    toolbar->setMovable(false);
    toolbar->setFloatable(false);
    toolbar->setContextMenuPolicy(Qt::PreventContextMenu);
    m_searchBar = new SearchBar(toolbar);
    m_searchBar->setMediaKind(config::Config::instance().searchKind());
    toolbar->addWidget(m_searchBar);
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

    // ---- Detail pane -------------------------------------------------------
    m_detailPane = new DetailPane(m_imageLoader, this);
    connect(m_detailPane, &DetailPane::copyMagnetRequested,
        this, &MainWindow::onCopyMagnet);
    connect(m_detailPane, &DetailPane::openMagnetRequested,
        this, &MainWindow::onOpenMagnet);
    connect(m_detailPane, &DetailPane::copyDirectUrlRequested,
        this, &MainWindow::onCopyDirectUrl);
    connect(m_detailPane, &DetailPane::openDirectUrlRequested,
        this, &MainWindow::onOpenDirectUrl);
    connect(m_detailPane, &DetailPane::episodeSelected,
        this, &MainWindow::onEpisodeSelected);

    // ---- Splitter root -----------------------------------------------------
    auto* splitter = new QSplitter(Qt::Horizontal, this);
    splitter->addWidget(m_resultsStack);
    splitter->addWidget(m_detailPane);
    splitter->setStretchFactor(0, 1);
    splitter->setStretchFactor(1, 2);
    splitter->setChildrenCollapsible(false);
    setCentralWidget(splitter);

    statusBar()->showMessage(i18nc("@info:status", "Ready"), 2000);
}

void MainWindow::buildActions()
{
    auto* fileMenu = menuBar()->addMenu(i18nc("@title:menu", "&File"));
    auto* quitAction = new QAction(
        QIcon::fromTheme(QStringLiteral("application-exit")),
        i18nc("@action:inmenu", "&Quit"), this);
    quitAction->setShortcut(QKeySequence::Quit);
    connect(quitAction, &QAction::triggered, qApp, &QApplication::quit);
    fileMenu->addAction(quitAction);

    auto* editMenu = menuBar()->addMenu(i18nc("@title:menu", "&Edit"));
    auto* focusSearch = new QAction(
        QIcon::fromTheme(QStringLiteral("search")),
        i18nc("@action:inmenu", "&Focus search"), this);
    focusSearch->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_K));
    connect(focusSearch, &QAction::triggered, this, [this] {
        m_searchBar->setFocus();
    });
    editMenu->addAction(focusSearch);

    auto* toolsMenu = menuBar()->addMenu(i18nc("@title:menu", "&Tools"));
    auto* rdAction = new QAction(
        QIcon::fromTheme(QStringLiteral("network-server")),
        i18nc("@action:inmenu", "&Real-Debrid\u2026"), this);
    rdAction->setShortcut(QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_R));
    connect(rdAction, &QAction::triggered, this, &MainWindow::showRealDebridDialog);
    toolsMenu->addAction(rdAction);

    auto* helpMenu = menuBar()->addMenu(i18nc("@title:menu", "&Help"));
    auto* aboutAction = new QAction(
        QIcon::fromTheme(QStringLiteral("help-about")),
        i18nc("@action:inmenu", "&About Kinema…"), this);
    connect(aboutAction, &QAction::triggered, this, &MainWindow::showAbout);
    helpMenu->addAction(aboutAction);
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
    m_detailPane->showMetaLoading();
    m_detailPane->showTorrentsLoading();

    try {
        auto sd = co_await m_cinemeta->seriesMeta(summary.imdbId);
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_detailPane->setSeries(sd);

        // Auto-select S1E1 (or the first non-special episode).
        const api::Episode* first = nullptr;
        for (const auto& e : sd.episodes) {
            if (e.season > 0) {
                first = &e;
                break;
            }
        }
        if (!first) {
            m_detailPane->showTorrentsEmpty();
            co_return;
        }
        auto t = loadEpisodeStreams(*first, summary.imdbId);
        Q_UNUSED(t);

    } catch (const core::HttpError& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "series detail failed:" << e.message();
        m_detailPane->showMetaError(e.message());
    } catch (const std::exception& e) {
        if (myEpoch != m_detailEpoch) {
            co_return;
        }
        m_detailPane->showMetaError(QString::fromUtf8(e.what()));
    }
}

QCoro::Task<void> MainWindow::loadEpisodeStreams(api::Episode episode, QString imdbId)
{
    const auto myEpoch = ++m_episodeEpoch;
    m_detailPane->showTorrentsLoading();

    const auto streamId = episode.streamId(imdbId);

    try {
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Series, streamId, currentConfig());
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_detailPane->setTorrents(std::move(streams));
    } catch (const core::HttpError& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        qCWarning(KINEMA) << "episode streams failed:" << e.message();
        m_detailPane->showTorrentsError(e.message());
    } catch (const std::exception& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_detailPane->showTorrentsError(QString::fromUtf8(e.what()));
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

void MainWindow::showRealDebridDialog()
{
    auto* dlg = new RealDebridDialog(m_http.get(), m_tokens.get(), this);
    dlg->setAttribute(Qt::WA_DeleteOnClose);
    connect(dlg, &RealDebridDialog::tokenChanged, this,
        [this](const QString& token) {
            m_rdToken = token;
            statusBar()->showMessage(
                token.isEmpty()
                    ? i18nc("@info:status", "Real-Debrid token removed.")
                    : i18nc("@info:status", "Real-Debrid token saved."),
                3000);
        });
    dlg->open();
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
