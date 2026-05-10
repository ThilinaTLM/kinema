// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Download.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "api/RealDebridClient.h"
#include "config/DownloadSettings.h"
#include "config/TorrentStreamingSettings.h"
#include "core/CachePaths.h"
#include "core/Database.h"
#include "core/DownloadStore.h"
#include "core/HttpClient.h"
#include "core/MediaCache.h"
#include "download/DownloadManager.h"
#include "torrent/TorrentStreamingService.h"

#include <KSharedConfig>

#include <QCoroSignal>
#include <QCoroTask>

#include <QDir>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

namespace {

// A no-op torrent engine. Inherits from `TorrentStreamingService`
// via the stub-only constructor so we never spin up a real
// libtorrent session. Records what the manager called.
class StubTorrentEngine final : public torrent::TorrentStreamingService
{
public:
    explicit StubTorrentEngine(QObject* parent = nullptr)
        : torrent::TorrentStreamingService(StubTag {}, parent)
    {
    }

    QCoro::Task<torrent::PreparedSession> prepareSession(
        const api::Stream& stream, const api::PlaybackContext& ctx,
        torrent::PrepareMode mode) override
    {
        Q_UNUSED(ctx);
        ++prepareCalls;
        lastMode = mode;
        lastInfoHash = stream.infoHash;
        torrent::PreparedSession ps;
        ps.token = QStringLiteral("tok-stub");
        ps.fileName = QStringLiteral("file.mkv");
        ps.fileSize = 1024LL * 1024LL * 1024LL;
        ps.infoHash = stream.infoHash;
        co_return ps;
    }

    QCoro::Task<QUrl> prepare(const api::Stream& s,
        const api::PlaybackContext& ctx) override
    {
        Q_UNUSED(s);
        Q_UNUSED(ctx);
        co_return QUrl();
    }

    void setKeepAlive(const QString& infoHash, bool on) override
    {
        keepAliveCalls.append(qMakePair(infoHash, on));
    }

    void promoteToFull(const QString& infoHash) override
    {
        promoteCalls.append(infoHash);
        // Match the real implementation's side effect so callers
        // observing keepAlive history don't have to know whether
        // promotion went through promoteToFull or setKeepAlive.
        keepAliveCalls.append(qMakePair(infoHash, true));
    }

    void pauseInfoHash(const QString& infoHash) override
    {
        pauseCalls.append(infoHash);
    }

    void resumeInfoHash(const QString& infoHash) override
    {
        resumeCalls.append(infoHash);
    }

    int prepareCalls = 0;
    torrent::PrepareMode lastMode = torrent::PrepareMode::Streaming;
    QString lastInfoHash;
    QList<QPair<QString, bool>> keepAliveCalls;
    QList<QString> promoteCalls;
    QList<QString> pauseCalls;
    QList<QString> resumeCalls;
};

api::Stream makeStream(const QString& infoHash = QString())
{
    api::Stream s;
    s.infoHash = infoHash.isEmpty()
        ? QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee")
        : infoHash;
    s.releaseName = QStringLiteral("Sample.Movie.2023.1080p");
    s.qualityLabel = QStringLiteral("Torrentio 1080p");
    s.resolution = QStringLiteral("1080p");
    s.provider = QStringLiteral("ThePirateBay");
    return s;
}

api::PlaybackContext makeContext(const QString& imdb = QStringLiteral("tt1234567"))
{
    api::PlaybackContext ctx;
    ctx.key.kind = api::MediaKind::Movie;
    ctx.key.imdbId = imdb;
    ctx.title = QStringLiteral("Sample Movie");
    return ctx;
}

} // namespace

class TstDownloadManager : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        // Wipe per-test caches so eviction doesn't carry over.
        QDir(core::cache::mediaDir()).removeRecursively();
        QDir().mkpath(core::cache::mediaDir().absolutePath());

        m_config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-dlmgr-test"),
            KConfig::SimpleConfig);
        m_dlSettings = std::make_unique<config::DownloadSettings>(m_config);
        m_dlSettings->setCacheBudgetGb(1);

        m_db = std::make_unique<core::Database>(
            QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::DownloadStore>(*m_db);
        m_cache = std::make_unique<core::MediaCache>(*m_dlSettings);
        m_http = std::make_unique<core::HttpClient>();
        m_rd = std::make_unique<api::RealDebridClient>(m_http.get());
        // Empty token ⇒ chooseBackend always picks Torrent for our
        // synthetic streams, which is what we want to exercise.
        m_rd->setToken(QString());
        m_engine = std::make_unique<StubTorrentEngine>();
        m_manager = std::make_unique<download::DownloadManager>(
            *m_http, *m_rd, *m_engine, *m_store, *m_cache,
            *m_dlSettings);
    }

    void cleanup()
    {
        m_manager.reset();
        m_engine.reset();
        m_rd.reset();
        m_http.reset();
        m_cache.reset();
        m_store.reset();
        m_db.reset();
        m_dlSettings.reset();
        QDir(core::cache::mediaDir()).removeRecursively();
    }

    /// Regression test for the headline bug: enqueueing a torrent
    /// download must actually call into the engine. Before the fix
    /// `enqueueDownload(Torrent)` only inserted a Queued row.
    void enqueueTorrentStartsBackgroundSession()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();

        m_manager->enqueueDownload(stream, ctx);

        // Spin the event loop until the QCoro task lands.
        for (int i = 0; i < 20 && m_engine->prepareCalls == 0; ++i) {
            QCoreApplication::processEvents();
        }

        QCOMPARE(m_engine->prepareCalls, 1);
        QCOMPARE(m_engine->lastMode, torrent::PrepareMode::Background);
        QCOMPARE(m_engine->lastInfoHash, stream.infoHash);

        // The row should have transitioned out of Queued. The
        // background helper persists Active once the session is
        // realised; allow either Resolving or Active depending on
        // how many event-loop turns have elapsed.
        const auto ref = api::assetRefFor(stream, ctx);
        const auto row = m_store->find(api::assetIdFor(ref));
        QVERIFY(row.has_value());
        QVERIFY(row->state == api::DownloadState::Active
            || row->state == api::DownloadState::Resolving);
        QCOMPARE(row->disposition, api::CacheDisposition::Pinned);
        QCOMPARE(row->mode, api::DownloadMode::Full);

        // Full ⇒ engine was told to keep the session alive.
        QVERIFY(!m_engine->keepAliveCalls.isEmpty());
        QCOMPARE(m_engine->keepAliveCalls.last().second, true);
    }

    void prepareForPlaybackThenDownloadUpgradesInPlace()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = api::assetIdFor(
            api::assetRefFor(stream, ctx));

        // Prime an OnDemand session via Play.
        QSignalSpy itemSpy(m_manager.get(),
            &download::DownloadManager::itemChanged);
        auto playTask = m_manager->prepareForPlayback(stream, ctx);
        Q_UNUSED(playTask);
        for (int i = 0; i < 20 && m_engine->prepareCalls == 0; ++i) {
            QCoreApplication::processEvents();
        }
        QCOMPARE(m_engine->prepareCalls, 1);
        QCOMPARE(m_engine->lastMode, torrent::PrepareMode::Streaming);

        const auto onDemandRow = m_store->find(assetId);
        QVERIFY(onDemandRow.has_value());
        QCOMPARE(onDemandRow->mode, api::DownloadMode::OnDemand);
        QCOMPARE(onDemandRow->disposition,
            api::CacheDisposition::Ephemeral);

        const int beforePrepareCalls = m_engine->prepareCalls;

        // Now click Download. Should upgrade in place; no second
        // prepareSession call, mode flips to Full, disposition to
        // Pinned, engine receives keepAlive=true.
        m_manager->enqueueDownload(stream, ctx);
        for (int i = 0; i < 5; ++i) {
            QCoreApplication::processEvents();
        }

        QCOMPARE(m_engine->prepareCalls, beforePrepareCalls);
        const auto upgraded = m_store->find(assetId);
        QVERIFY(upgraded.has_value());
        QCOMPARE(upgraded->mode, api::DownloadMode::Full);
        QCOMPARE(upgraded->disposition,
            api::CacheDisposition::Pinned);
        QVERIFY(!m_engine->promoteCalls.isEmpty());
        QCOMPARE(m_engine->promoteCalls.last(), stream.infoHash);
    }

    void pauseAndResumeRouteToEngine()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = api::assetIdFor(
            api::assetRefFor(stream, ctx));

        m_manager->enqueueDownload(stream, ctx);
        for (int i = 0; i < 20 && m_engine->prepareCalls == 0; ++i) {
            QCoreApplication::processEvents();
        }

        m_manager->pause(assetId);
        QCOMPARE(m_engine->pauseCalls.size(), 1);
        QCOMPARE(m_engine->pauseCalls.last(), stream.infoHash);
        const auto pausedRow = m_store->find(assetId);
        QVERIFY(pausedRow.has_value());
        QCOMPARE(pausedRow->state, api::DownloadState::Paused);

        m_manager->resume(assetId);
        QCOMPARE(m_engine->resumeCalls.size(), 1);
        QCOMPARE(m_engine->resumeCalls.last(), stream.infoHash);
        const auto resumedRow = m_store->find(assetId);
        QVERIFY(resumedRow.has_value());
        QCOMPARE(resumedRow->state, api::DownloadState::Active);
    }

    void resumePersistedRestartsOnlyFullRows()
    {
        // Pre-populate the store directly so we don't realise any
        // sessions through the manager.
        api::DownloadItem fullActive;
        fullActive.assetId = QStringLiteral("asset-full-active");
        fullActive.backendKind = api::DownloadBackendKind::Torrent;
        fullActive.state = api::DownloadState::Active;
        fullActive.mode = api::DownloadMode::Full;
        fullActive.disposition = api::CacheDisposition::Pinned;
        fullActive.key.kind = api::MediaKind::Movie;
        fullActive.key.imdbId = QStringLiteral("tt9000001");
        fullActive.title = QStringLiteral("Full Active");
        fullActive.infoHash = QStringLiteral(
            "1111222233334444555566667777888899990000");
        m_store->upsert(fullActive);

        api::DownloadItem fullCompleted;
        fullCompleted.assetId = QStringLiteral("asset-full-done");
        fullCompleted.backendKind = api::DownloadBackendKind::Torrent;
        fullCompleted.state = api::DownloadState::Completed;
        fullCompleted.mode = api::DownloadMode::Full;
        fullCompleted.disposition = api::CacheDisposition::Pinned;
        fullCompleted.key.kind = api::MediaKind::Movie;
        fullCompleted.key.imdbId = QStringLiteral("tt9000002");
        fullCompleted.title = QStringLiteral("Full Done");
        fullCompleted.infoHash = QStringLiteral(
            "aaaa222233334444555566667777888899990000");
        m_store->upsert(fullCompleted);

        api::DownloadItem onDemandActive;
        onDemandActive.assetId = QStringLiteral("asset-od-active");
        onDemandActive.backendKind = api::DownloadBackendKind::Torrent;
        onDemandActive.state = api::DownloadState::Active;
        onDemandActive.mode = api::DownloadMode::OnDemand;
        onDemandActive.disposition = api::CacheDisposition::Ephemeral;
        onDemandActive.key.kind = api::MediaKind::Movie;
        onDemandActive.key.imdbId = QStringLiteral("tt9000003");
        onDemandActive.title = QStringLiteral("OnDemand Active");
        onDemandActive.infoHash = QStringLiteral(
            "bbbb222233334444555566667777888899990000");
        m_store->upsert(onDemandActive);

        const int beforeCalls = m_engine->prepareCalls;
        m_manager->resumePersisted();
        for (int i = 0; i < 20
            && m_engine->prepareCalls == beforeCalls; ++i) {
            QCoreApplication::processEvents();
        }

        // Exactly one resumed row -> exactly one prepareSession call.
        QCOMPARE(m_engine->prepareCalls, beforeCalls + 1);
        QCOMPARE(m_engine->lastInfoHash, fullActive.infoHash);
        QCOMPARE(m_engine->lastMode, torrent::PrepareMode::Background);
    }

    void enqueueRejectsStreamsWithoutInfoHash()
    {
        api::Stream s;
        // No info hash, no direct URL.
        s.releaseName = QStringLiteral("Junk");
        QSignalSpy statusSpy(m_manager.get(),
            &download::DownloadManager::statusMessage);

        m_manager->enqueueDownload(s, makeContext());

        QCOMPARE(m_engine->prepareCalls, 0);
        QCOMPARE(m_store->loadAll().size(), 0);
        QVERIFY(statusSpy.count() >= 1);
    }

    /// retry() on a previously failed torrent row must re-arm the
    /// engine. Before the fix it only flipped the row state.
    void retryRestartsTorrentSession()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = api::assetIdFor(api::assetRefFor(stream, ctx));

        m_manager->enqueueDownload(stream, ctx);
        for (int i = 0; i < 20 && m_engine->prepareCalls == 0; ++i) {
            QCoreApplication::processEvents();
        }
        QCOMPARE(m_engine->prepareCalls, 1);

        // Simulate a fault.
        m_store->updateState(assetId, api::DownloadState::Failed,
            QStringLiteral("network error"));

        m_manager->retry(assetId);
        for (int i = 0; i < 20 && m_engine->prepareCalls < 2; ++i) {
            QCoreApplication::processEvents();
        }
        QCOMPARE(m_engine->prepareCalls, 2);
        const auto row = m_store->find(assetId);
        QVERIFY(row.has_value());
        QVERIFY(row->state != api::DownloadState::Failed);
        QVERIFY(row->lastError.isEmpty());
    }

    void cancelMarksRowAndStopsHash()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = api::assetIdFor(api::assetRefFor(stream, ctx));

        m_manager->enqueueDownload(stream, ctx);
        for (int i = 0; i < 20 && m_engine->prepareCalls == 0; ++i) {
            QCoreApplication::processEvents();
        }

        m_manager->cancel(assetId);
        const auto row = m_store->find(assetId);
        QVERIFY(row.has_value());
        QCOMPARE(row->state, api::DownloadState::Cancelled);
        // Live stats should be cleared.
        QVERIFY(!m_manager->liveStatsFor(assetId).has_value());
    }

    void pinPropagatesToEngine()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = api::assetIdFor(api::assetRefFor(stream, ctx));

        // Use Play (OnDemand+Ephemeral) so the keepAlive history
        // starts clean; otherwise enqueueDownload's own Pinned
        // upsert would have already pushed (hash, true) onto the
        // engine.
        auto playTask = m_manager->prepareForPlayback(stream, ctx);
        Q_UNUSED(playTask);
        for (int i = 0; i < 20 && m_engine->prepareCalls == 0; ++i) {
            QCoreApplication::processEvents();
        }
        const int beforeCalls = m_engine->keepAliveCalls.size();

        m_manager->pin(assetId, true);
        QVERIFY(m_engine->keepAliveCalls.size() > beforeCalls);
        QCOMPARE(m_engine->keepAliveCalls.last().first, stream.infoHash);
        QCOMPARE(m_engine->keepAliveCalls.last().second, true);

        m_manager->pin(assetId, false);
        QCOMPARE(m_engine->keepAliveCalls.last().second, false);
    }

private:
    KSharedConfigPtr m_config;
    std::unique_ptr<config::DownloadSettings> m_dlSettings;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::DownloadStore> m_store;
    std::unique_ptr<core::MediaCache> m_cache;
    std::unique_ptr<core::HttpClient> m_http;
    std::unique_ptr<api::RealDebridClient> m_rd;
    std::unique_ptr<StubTorrentEngine> m_engine;
    std::unique_ptr<download::DownloadManager> m_manager;
};

QTEST_MAIN(TstDownloadManager)
#include "tst_download_manager.moc"
