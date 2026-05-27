// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/DownloadSettings.h"
#include "core/io/CachePaths.h"
#include "core/persistence/MediaCache.h"
#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "download/AssetSession.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/DownloadRepository.h"
#include "playback/ports/MediaSourcePort.h"
#include "playback/streaming/LocalHttpStreamGateway.h"
#include "playback/transfer/BackendRegistry.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSession.h"
#include "playback/transfer/TransferSupervisor.h"
#include "playback/transfer/TransferUseCase.h"
#include "torrent/TorrentStreamingService.h"

#include <KSharedConfig>

#include <QCoroSignal>
#include <QCoroTask>

#include <QDir>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

#include <map>
#include <memory>
#include <utility>

using namespace kinema;
using namespace kinema::playback::transfer;
namespace events = kinema::playback::events;
namespace ports = kinema::playback::ports;
namespace streaming = kinema::playback::streaming;

namespace {

// ---------------------------------------------------------------------------
// FakeAssetSession — a `download::AssetSession` subclass we can drive from
// tests. Mirrors the helper in `tst_transfer_supervisor.cpp` so the
// supervisor's progress wiring keeps working end-to-end.
// ---------------------------------------------------------------------------
class FakeAssetSession final : public download::AssetSession
{
    Q_OBJECT
public:
    FakeAssetSession(QString assetId, QString fileName, qint64 fileSize,
        qint64 initialCached, QObject* parent = nullptr)
        : download::AssetSession(parent)
        , m_assetId(std::move(assetId))
        , m_fileName(std::move(fileName))
        , m_fileSize(fileSize)
        , m_cached(initialCached)
    {
    }

    QString token() const override { return m_assetId; }
    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_fileSize; }
    qint64 cachedBytes() const override { return m_cached; }
    QCoro::Task<bool> ensureRange(kinema::torrent::ByteRange) override
    {
        co_return true;
    }
    QByteArray readRange(kinema::torrent::ByteRange) const override
    {
        return {};
    }
    void touch() override { ++touchCalls; }
    void pause() override { ++pauseCalls; }
    void resume() override { ++resumeCalls; }
    domain::DownloadMode mode() const override { return m_mode; }
    void setMode(domain::DownloadMode m) override { m_mode = m; }

    int touchCalls = 0;
    int pauseCalls = 0;
    int resumeCalls = 0;

private:
    QString m_assetId;
    QString m_fileName;
    qint64 m_fileSize;
    qint64 m_cached;
    domain::DownloadMode m_mode = domain::DownloadMode::OnDemand;
};

// ---------------------------------------------------------------------------
// FakeMediaSourcePort — emits a `FakeAssetSession` on each open(). Records
// open / changeMode call counts for assertions.
// ---------------------------------------------------------------------------
class FakeMediaSourcePort : public ports::MediaSourcePort
{
public:
    FakeMediaSourcePort(domain::DownloadBackendKind kind,
        qint64 fileSize, qint64 initialCached = 0)
        : m_kind(kind)
        , m_fileSize(fileSize)
        , m_initialCached(initialCached)
    {
    }

    domain::DownloadBackendKind kind() const noexcept override
    {
        return m_kind;
    }

    bool canHandle(const domain::Stream& s) const override
    {
        return !s.infoHash.isEmpty();
    }

    QCoro::Task<ports::OpenedSession> open(const domain::AssetRef& ref,
        const domain::Stream&, const domain::PlaybackContext&,
        domain::DownloadMode mode) override
    {
        ++openCalls;
        lastMode = mode;
        const auto assetId = domain::assetIdFor(ref);
        auto session = std::make_unique<FakeAssetSession>(assetId,
            QStringLiteral("episode.mkv"), m_fileSize,
            m_initialCached);
        session->setMode(mode);
        lastOpenedAssetId = assetId;
        ports::OpenedSession out;
        out.assetId = assetId;
        out.session = std::move(session);
        co_return out;
    }

    void changeMode(ports::ByteRangeSource& session,
        domain::DownloadMode newMode) override
    {
        ++changeModeCalls;
        lastChangeMode = newMode;
        if (auto* s = dynamic_cast<FakeAssetSession*>(&session)) {
            s->setMode(newMode);
        }
    }

    int openCalls = 0;
    int changeModeCalls = 0;
    domain::DownloadMode lastMode = domain::DownloadMode::OnDemand;
    domain::DownloadMode lastChangeMode = domain::DownloadMode::OnDemand;
    QString lastOpenedAssetId;

private:
    domain::DownloadBackendKind m_kind;
    qint64 m_fileSize;
    qint64 m_initialCached;
};

// ---------------------------------------------------------------------------
// FakeDownloadRepo — in-memory `DownloadRepository`. Mirrors the helper in
// `tst_transfer_supervisor.cpp`.
// ---------------------------------------------------------------------------
class FakeDownloadRepo final : public ports::DownloadRepository
{
public:
    void upsert(const domain::DownloadItem& item) override
    {
        m_rows[item.assetId] = item;
    }
    void updateState(const QString& assetId,
        domain::DownloadState state) override
    {
        if (auto it = m_rows.find(assetId); it != m_rows.end()) {
            it->second.state = state;
        }
    }
    void updateCachedBytes(const QString& assetId, qint64 cachedBytes,
        std::optional<qint64> expectedBytes, bool complete) override
    {
        if (auto it = m_rows.find(assetId); it != m_rows.end()) {
            it->second.cachedSizeBytes = cachedBytes;
            it->second.complete = complete;
            if (expectedBytes) {
                it->second.expectedSizeBytes = *expectedBytes;
            }
        }
    }
    void setLastError(const QString& assetId,
        const QString& error) override
    {
        if (auto it = m_rows.find(assetId); it != m_rows.end()) {
            it->second.lastError = error;
            it->second.state = domain::DownloadState::Failed;
        }
    }
    void updateMode(const QString& assetId,
        domain::DownloadMode mode) override
    {
        if (auto it = m_rows.find(assetId); it != m_rows.end()) {
            it->second.mode = mode;
        }
    }
    void setDisposition(const QString& assetId,
        domain::CacheDisposition disposition) override
    {
        if (auto it = m_rows.find(assetId); it != m_rows.end()) {
            it->second.disposition = disposition;
        }
    }
    void remove(const QString& assetId) override
    {
        m_rows.erase(assetId);
    }
    std::optional<domain::DownloadItem> find(
        const QString& assetId) const override
    {
        const auto it = m_rows.find(assetId);
        if (it == m_rows.end()) {
            return std::nullopt;
        }
        return it->second;
    }
    std::optional<domain::DownloadItem> findForKey(
        const domain::PlaybackKey& key) const override
    {
        for (const auto& [id, row] : m_rows) {
            Q_UNUSED(id);
            if (row.key == key) {
                return row;
            }
        }
        return std::nullopt;
    }
    QVector<domain::DownloadItem> all() const override
    {
        QVector<domain::DownloadItem> out;
        out.reserve(static_cast<int>(m_rows.size()));
        for (const auto& [id, row] : m_rows) {
            Q_UNUSED(id);
            out.append(row);
        }
        return out;
    }

    void seedRow(const domain::DownloadItem& row)
    {
        m_rows[row.assetId] = row;
    }

private:
    std::map<QString, domain::DownloadItem> m_rows;
};

// ---------------------------------------------------------------------------
// StubTorrentEngine — drop-in for `kinema::torrent::TorrentStreamingService`.
// Lifted from `tst_download_manager.cpp`. Records lifecycle calls only.
// ---------------------------------------------------------------------------
class StubTorrentEngine final
    : public kinema::torrent::TorrentStreamingService
{
public:
    explicit StubTorrentEngine(QObject* parent = nullptr)
        : kinema::torrent::TorrentStreamingService(StubTag {}, parent)
    {
    }

    QCoro::Task<kinema::torrent::PreparedSession> prepareSession(
        const domain::Stream&, const domain::PlaybackContext&,
        kinema::torrent::PrepareMode) override
    {
        kinema::torrent::PreparedSession ps;
        co_return ps;
    }
    QCoro::Task<QUrl> prepare(const domain::Stream&,
        const domain::PlaybackContext&) override
    {
        co_return QUrl();
    }
    void setKeepAlive(const QString& infoHash, bool on) override
    {
        keepAliveCalls.append({ infoHash, on });
    }

    QList<QPair<QString, bool>> keepAliveCalls;
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

domain::Stream makeStream(const QString& infoHash = {})
{
    domain::Stream s;
    s.infoHash = infoHash.isEmpty()
        ? QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee")
        : infoHash;
    s.releaseName = QStringLiteral("Sample.Movie.2024.1080p");
    s.qualityLabel = QStringLiteral("1080p");
    return s;
}

domain::PlaybackContext makeContext(const QString& imdb
    = QStringLiteral("tt7654321"))
{
    domain::PlaybackContext ctx;
    ctx.key.kind = domain::MediaKind::Movie;
    ctx.key.imdbId = imdb;
    ctx.title = QStringLiteral("Sample Movie");
    return ctx;
}

void spin(int turns = 30)
{
    for (int i = 0; i < turns; ++i) {
        QCoreApplication::processEvents();
    }
}

template <typename Cond>
void spinUntil(Cond&& cond, int maxTurns = 50)
{
    for (int i = 0; i < maxTurns; ++i) {
        if (cond()) {
            return;
        }
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstTransferUseCase : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        // Wipe per-test caches so eviction / pinned markers don't
        // bleed between cases.
        QDir(core::cache::mediaDir()).removeRecursively();
        QDir().mkpath(core::cache::mediaDir().absolutePath());

        m_config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-tuc-test"),
            KConfig::SimpleConfig);
        m_dlSettings = std::make_unique<config::DownloadSettings>(m_config);
        m_dlSettings->setCacheBudgetGb(1);

        m_cache = std::make_unique<core::MediaCache>(*m_dlSettings);
        m_engine = std::make_unique<StubTorrentEngine>();

        m_backends = std::make_unique<BackendRegistry>();
        m_torrentSource
            = new FakeMediaSourcePort(domain::DownloadBackendKind::Torrent,
                /*fileSize*/ 2'000'000);
        m_backends->registerSource(
            std::unique_ptr<ports::MediaSourcePort>(m_torrentSource));

        m_sessions = std::make_unique<SessionRegistry>();
        m_repo = std::make_unique<FakeDownloadRepo>();
        m_events = std::make_unique<events::PlaybackEventStream>();
        m_supervisor = std::make_unique<TransferSupervisor>(
            *m_sessions, *m_repo, *m_events);
        m_gateway
            = std::make_unique<streaming::LocalHttpStreamGateway>();
        QVERIFY(m_gateway->listen());

        m_useCase = std::make_unique<TransferUseCase>(*m_backends,
            *m_sessions, *m_supervisor, *m_gateway, *m_repo, *m_cache,
            *m_engine);
    }

    void cleanup()
    {
        m_useCase.reset();
        m_gateway.reset();
        m_supervisor.reset();
        m_events.reset();
        m_repo.reset();
        m_sessions.reset();
        m_backends.reset();
        m_torrentSource = nullptr;
        m_engine.reset();
        m_cache.reset();
        m_dlSettings.reset();
        QDir(core::cache::mediaDir()).removeRecursively();
    }

    // -----------------------------------------------------------------
    // ensurePlayable round-trips through the fake `MediaSourcePort`
    // and registers a live session in the registry.
    // -----------------------------------------------------------------
    void ensurePlayableOpensSessionAndReturnsLocalhostUrl()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        QUrl url;
        bool done = false;
        auto task = [&]() -> QCoro::Task<void> {
            url = co_await m_useCase->ensurePlayable(stream, ctx);
            done = true;
            co_return;
        }();
        spinUntil([&] { return done; });
        QVERIFY(done);
        QVERIFY(url.isValid());
        QCOMPARE(url.scheme(), QStringLiteral("http"));
        QCOMPARE(url.host(), QStringLiteral("127.0.0.1"));

        // Backend was asked to open one session in OnDemand mode.
        QCOMPARE(m_torrentSource->openCalls, 1);
        QCOMPARE(m_torrentSource->lastMode,
            domain::DownloadMode::OnDemand);

        // Registry now holds a session for the asset; player is
        // attached.
        QVERIFY(m_sessions->contains(assetId));
        QVERIFY(m_useCase->attachedPlayerAssetIds().contains(assetId));

        // Persisted row promoted to Active by openSession.
        const auto row = m_repo->find(assetId);
        QVERIFY(row.has_value());
        QCOMPARE(row->state, domain::DownloadState::Active);
        QCOMPARE(row->mode, domain::DownloadMode::OnDemand);
        QCOMPARE(row->disposition, domain::CacheDisposition::Ephemeral);
    }

    // -----------------------------------------------------------------
    // saveOffline opens with Full+Pinned and persists the row.
    // -----------------------------------------------------------------
    void saveOfflineOpensFullPinnedAndPersistsRow()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        m_useCase->saveOffline(stream, ctx);
        spinUntil([&] { return m_torrentSource->openCalls >= 1; });

        QCOMPARE(m_torrentSource->openCalls, 1);
        QCOMPARE(m_torrentSource->lastMode, domain::DownloadMode::Full);

        const auto row = m_repo->find(assetId);
        QVERIFY(row.has_value());
        QCOMPARE(row->mode, domain::DownloadMode::Full);
        QCOMPARE(row->disposition, domain::CacheDisposition::Pinned);
        QVERIFY(m_cache->isPinned(assetId));
    }

    // -----------------------------------------------------------------
    // saveOffline on an already-active session must upgrade in place,
    // not spawn a duplicate.
    // -----------------------------------------------------------------
    void saveOfflineUpgradesExistingOnDemandSession()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        // Prime with an OnDemand play.
        bool playDone = false;
        auto play = [&]() -> QCoro::Task<void> {
            co_await m_useCase->ensurePlayable(stream, ctx);
            playDone = true;
        }();
        spinUntil([&] { return playDone; });
        QCOMPARE(m_torrentSource->openCalls, 1);

        // Now save-offline: must upgrade in place (no second open).
        m_useCase->saveOffline(stream, ctx);
        spin(5);
        QCOMPARE(m_torrentSource->openCalls, 1);
        QCOMPARE(m_torrentSource->changeModeCalls, 1);
        QCOMPARE(m_torrentSource->lastChangeMode,
            domain::DownloadMode::Full);

        const auto row = m_repo->find(assetId);
        QVERIFY(row.has_value());
        QCOMPARE(row->mode, domain::DownloadMode::Full);
        QCOMPARE(row->disposition, domain::CacheDisposition::Pinned);
        QVERIFY(m_cache->isPinned(assetId));
    }

    // -----------------------------------------------------------------
    // attachPlayer flips an Idle row to Active.
    // -----------------------------------------------------------------
    void attachPlayerPromotesIdleRowToActive()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        bool playDone = false;
        auto play = [&]() -> QCoro::Task<void> {
            co_await m_useCase->ensurePlayable(stream, ctx);
            playDone = true;
        }();
        spinUntil([&] { return playDone; });

        // Detach to park the row at Idle (OnDemand contract).
        m_useCase->detachPlayer(assetId);
        const auto parked = m_repo->find(assetId);
        QVERIFY(parked.has_value());
        QCOMPARE(parked->state, domain::DownloadState::Idle);

        // Reattach — row returns to Active.
        m_useCase->attachPlayer(assetId);
        const auto promoted = m_repo->find(assetId);
        QVERIFY(promoted.has_value());
        QCOMPARE(promoted->state, domain::DownloadState::Active);
    }

    // -----------------------------------------------------------------
    // cancel revokes the session, marks the row Cancelled, and stops
    // the torrent engine for the info hash.
    // -----------------------------------------------------------------
    void cancelStopsSessionAndMarksRow()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        m_useCase->saveOffline(stream, ctx);
        spinUntil([&] { return m_torrentSource->openCalls >= 1; });
        QVERIFY(m_sessions->contains(assetId));

        m_useCase->cancel(assetId);
        QVERIFY(!m_sessions->contains(assetId));
        const auto row = m_repo->find(assetId);
        QVERIFY(row.has_value());
        QCOMPARE(row->state, domain::DownloadState::Cancelled);
    }

    // -----------------------------------------------------------------
    // remove(deleteFiles=true) removes the row entirely + clears
    // pinned marker + asks the engine to stop.
    // -----------------------------------------------------------------
    void removeDeletesRowAndClearsPin()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        m_useCase->saveOffline(stream, ctx);
        spinUntil([&] { return m_torrentSource->openCalls >= 1; });
        QVERIFY(m_cache->isPinned(assetId));

        m_useCase->remove(assetId, /*deleteFiles*/ true);
        QVERIFY(!m_sessions->contains(assetId));
        QVERIFY(!m_repo->find(assetId).has_value());
        QVERIFY(!m_cache->isPinned(assetId));
    }

    // -----------------------------------------------------------------
    // retry of a Failed row re-fires open with the original mode.
    // -----------------------------------------------------------------
    void retryRestartsFromPersistedRow()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        m_useCase->saveOffline(stream, ctx);
        spinUntil([&] { return m_torrentSource->openCalls >= 1; });
        QCOMPARE(m_torrentSource->openCalls, 1);

        // Forcibly mark it failed and erase the live session so
        // retry runs the cold path through the repo.
        m_useCase->cancel(assetId);
        m_repo->seedRow([&] {
            auto row = *m_repo->find(assetId);
            row.state = domain::DownloadState::Failed;
            return row;
        }());

        m_useCase->retry(assetId);
        spinUntil([&] { return m_torrentSource->openCalls >= 2; });
        QCOMPARE(m_torrentSource->openCalls, 2);
        QCOMPARE(m_torrentSource->lastMode, domain::DownloadMode::Full);
    }

    // -----------------------------------------------------------------
    // pin toggles the cache marker + the persisted disposition +
    // forwards keepAlive to the torrent engine.
    // -----------------------------------------------------------------
    void pinTogglesMarkerAndKeepAlive()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        bool playDone = false;
        auto play = [&]() -> QCoro::Task<void> {
            co_await m_useCase->ensurePlayable(stream, ctx);
            playDone = true;
        }();
        spinUntil([&] { return playDone; });

        const int before = m_engine->keepAliveCalls.size();
        m_useCase->pin(assetId, true);
        QVERIFY(m_engine->keepAliveCalls.size() > before);
        QCOMPARE(m_engine->keepAliveCalls.last(),
            qMakePair(stream.infoHash, true));
        QVERIFY(m_cache->isPinned(assetId));
        QCOMPARE(m_repo->find(assetId)->disposition,
            domain::CacheDisposition::Pinned);

        m_useCase->pin(assetId, false);
        QCOMPARE(m_engine->keepAliveCalls.last(),
            qMakePair(stream.infoHash, false));
        QVERIFY(!m_cache->isPinned(assetId));
    }

    // -----------------------------------------------------------------
    // pause / resume forward to the underlying session + repo.
    // -----------------------------------------------------------------
    void pauseAndResumeForwardToSessionAndRepo()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        bool playDone = false;
        auto play = [&]() -> QCoro::Task<void> {
            co_await m_useCase->ensurePlayable(stream, ctx);
            playDone = true;
        }();
        spinUntil([&] { return playDone; });

        m_useCase->pause(assetId);
        QCOMPARE(m_repo->find(assetId)->state,
            domain::DownloadState::Paused);
        auto* fake = dynamic_cast<FakeAssetSession*>(
            m_sessions->find(assetId)->legacySession());
        QVERIFY(fake);
        QCOMPARE(fake->pauseCalls, 1);

        m_useCase->resumeTransfer(assetId);
        QCOMPARE(m_repo->find(assetId)->state,
            domain::DownloadState::Active);
        QCOMPARE(fake->resumeCalls, 1);
    }

    // -----------------------------------------------------------------
    // resumePersisted re-fires every Full + non-terminal row.
    // -----------------------------------------------------------------
    void resumePersistedReFiresOnlyFullNonTerminalRows()
    {
        // Pre-seed three rows: one Full+Active, one Full+Completed
        // (terminal), one OnDemand+Active. Only the first should be
        // resumed.
        domain::DownloadItem fullActive;
        fullActive.assetId = QStringLiteral("a-1");
        fullActive.backendKind = domain::DownloadBackendKind::Torrent;
        fullActive.state = domain::DownloadState::Active;
        fullActive.mode = domain::DownloadMode::Full;
        fullActive.disposition = domain::CacheDisposition::Pinned;
        fullActive.key.kind = domain::MediaKind::Movie;
        fullActive.key.imdbId = QStringLiteral("tt0000001");
        fullActive.infoHash = QStringLiteral(
            "1111222233334444555566667777888899990001");
        m_repo->seedRow(fullActive);

        domain::DownloadItem fullDone = fullActive;
        fullDone.assetId = QStringLiteral("a-2");
        fullDone.state = domain::DownloadState::Completed;
        fullDone.key.imdbId = QStringLiteral("tt0000002");
        fullDone.infoHash = QStringLiteral(
            "1111222233334444555566667777888899990002");
        m_repo->seedRow(fullDone);

        domain::DownloadItem onDemandActive = fullActive;
        onDemandActive.assetId = QStringLiteral("a-3");
        onDemandActive.mode = domain::DownloadMode::OnDemand;
        onDemandActive.disposition = domain::CacheDisposition::Ephemeral;
        onDemandActive.key.imdbId = QStringLiteral("tt0000003");
        onDemandActive.infoHash = QStringLiteral(
            "1111222233334444555566667777888899990003");
        m_repo->seedRow(onDemandActive);

        m_useCase->resumePersisted();
        spinUntil([&] { return m_torrentSource->openCalls >= 1; });
        QCOMPARE(m_torrentSource->openCalls, 1);
        QCOMPARE(m_torrentSource->lastMode, domain::DownloadMode::Full);
    }

    // -----------------------------------------------------------------
    // Same-infoHash supersede: a second open() on a different
    // assetId but the same hash revokes the previous session.
    // -----------------------------------------------------------------
    void sameInfoHashSupersedesPriorSession()
    {
        // Mimic series-pack episode swap: same infoHash, two
        // different file indices inside the pack. assetIdFor()
        // appends `-f<fileIndex>` so the two assetIds differ even
        // though the underlying torrent is the same.
        auto streamA = makeStream();
        streamA.fileIndex = 0;
        streamA.fileNameHint = QStringLiteral("S01E01.mkv");
        auto streamB = makeStream();
        streamB.fileIndex = 1;
        streamB.fileNameHint = QStringLiteral("S01E02.mkv");

        domain::PlaybackContext ctxA;
        ctxA.key.kind = domain::MediaKind::Series;
        ctxA.key.imdbId = QStringLiteral("tt5555555");
        ctxA.key.season = 1;
        ctxA.key.episode = 1;
        ctxA.title = QStringLiteral("Show S01E01");

        domain::PlaybackContext ctxB = ctxA;
        ctxB.key.episode = 2;
        ctxB.title = QStringLiteral("Show S01E02");

        const auto assetA = domain::assetIdFor(
            domain::assetRefFor(streamA, ctxA));
        const auto assetB = domain::assetIdFor(
            domain::assetRefFor(streamB, ctxB));
        QVERIFY(assetA != assetB);

        bool aDone = false;
        auto playA = [&]() -> QCoro::Task<void> {
            co_await m_useCase->ensurePlayable(streamA, ctxA);
            aDone = true;
        }();
        spinUntil([&] { return aDone; });
        QVERIFY(m_sessions->contains(assetA));

        bool bDone = false;
        auto playB = [&]() -> QCoro::Task<void> {
            co_await m_useCase->ensurePlayable(streamB, ctxB);
            bDone = true;
        }();
        spinUntil([&] { return bDone; });
        QVERIFY(m_sessions->contains(assetB));
        // The earlier session was revoked.
        QVERIFY(!m_sessions->contains(assetA));
    }

    // -----------------------------------------------------------------
    // SessionFileCatalog: files lookup via streamRef and assetId.
    // -----------------------------------------------------------------
    void filesForStreamRefAndAssetIdRouteThroughRegistry()
    {
        const auto stream = makeStream();
        const auto ctx = makeContext();
        const auto assetId = domain::assetIdFor(
            domain::assetRefFor(stream, ctx));

        bool done = false;
        auto play = [&]() -> QCoro::Task<void> {
            co_await m_useCase->ensurePlayable(stream, ctx);
            done = true;
        }();
        spinUntil([&] { return done; });

        // FakeAssetSession::files() returns an empty vector, so we
        // assert the call routes through cleanly rather than asserting
        // contents. The supervisor's existing tests cover content
        // semantics.
        domain::HistoryStreamRef ref;
        ref.infoHash = stream.infoHash;
        const auto byRef = m_useCase->filesForStreamRef(ref);
        const auto byId = m_useCase->filesForAssetId(assetId);
        QCOMPARE(byRef.size(), 0);
        QCOMPARE(byId.size(), 0);

        // Empty streamRef gracefully short-circuits.
        QCOMPARE(m_useCase->filesForStreamRef({}).size(), 0);
    }

    // -----------------------------------------------------------------
    // ensurePlayable on a stream without info hash throws the
    // "no playable info hash" runtime_error.
    // -----------------------------------------------------------------
    void ensurePlayableThrowsOnInvalidStream()
    {
        domain::Stream s;
        // No info hash, no direct URL → not a valid AssetRef.

        bool threw = false;
        bool done = false;
        auto task = [&]() -> QCoro::Task<void> {
            try {
                co_await m_useCase->ensurePlayable(s, makeContext());
            } catch (const std::exception&) {
                threw = true;
            }
            done = true;
            co_return;
        }();
        spinUntil([&] { return done; });
        QVERIFY(threw);
    }

private:
    KSharedConfig::Ptr m_config;
    std::unique_ptr<config::DownloadSettings> m_dlSettings;
    std::unique_ptr<core::MediaCache> m_cache;
    std::unique_ptr<StubTorrentEngine> m_engine;
    std::unique_ptr<BackendRegistry> m_backends;
    FakeMediaSourcePort* m_torrentSource = nullptr; // owned by m_backends
    std::unique_ptr<SessionRegistry> m_sessions;
    std::unique_ptr<FakeDownloadRepo> m_repo;
    std::unique_ptr<events::PlaybackEventStream> m_events;
    std::unique_ptr<TransferSupervisor> m_supervisor;
    std::unique_ptr<streaming::LocalHttpStreamGateway> m_gateway;
    std::unique_ptr<TransferUseCase> m_useCase;
};

QTEST_MAIN(TstTransferUseCase)
#include "tst_transfer_use_case.moc"
