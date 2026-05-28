// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Download.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/sources/AssetSession.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/DownloadRepository.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSession.h"
#include "playback/transfer/TransferSupervisor.h"
#include "torrent/TorrentFileEntry.h"

#include <QByteArray>
#include <QCoro/QCoroTask>
#include <QSignalSpy>
#include <QString>
#include <QTest>
#include <QUuid>
#include <QVector>

#include <memory>
#include <vector>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::events;
using namespace kinema::playback::transfer;

namespace {

class FakeAssetSession final : public playback::sources::AssetSession
{
    Q_OBJECT
public:
    FakeAssetSession(QString assetId,
        QString fileName,
        qint64 fileSize,
        QObject* parent = nullptr)
        : playback::sources::AssetSession(parent)
        , m_assetId(std::move(assetId))
        , m_fileName(std::move(fileName))
        , m_fileSize(fileSize)
    {
    }
    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_fileSize; }
    qint64 cachedBytes() const override { return m_cached; }
    QCoro::Task<bool> ensureRange(torrent::ByteRange) override
    {
        co_return true;
    }
    QByteArray readRange(torrent::ByteRange) const override { return {}; }
    void touch() override {}
    domain::DownloadMode mode() const override { return m_mode; }
    void setMode(domain::DownloadMode m) override { m_mode = m; }

    void emitCachedBytes(qint64 b)
    {
        m_cached = b;
        Q_EMIT cachedBytesChanged(b);
    }
    void emitCompleted() { Q_EMIT completed(); }
    void emitFailed(const QString& r) { Q_EMIT failed(r); }
    void emitLiveStats(qint64 rate, int peers, int seeds, int eta)
    {
        Q_EMIT liveStatsChanged(rate, peers, seeds, eta);
    }

private:
    QString m_assetId;
    QString m_fileName;
    qint64 m_fileSize;
    qint64 m_cached = 0;
    domain::DownloadMode m_mode = domain::DownloadMode::OnDemand;
};

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
        ++stateCalls;
        lastStateAssetId = assetId;
        lastState = state;
        if (auto it = m_rows.find(assetId); it != m_rows.end()) {
            it->second.state = state;
        }
    }
    void updateCachedBytes(const QString& assetId,
        qint64 cachedBytes,
        std::optional<qint64> expectedBytes,
        bool complete) override
    {
        ++cachedCalls;
        lastCachedAssetId = assetId;
        lastCachedBytes = cachedBytes;
        lastExpectedBytes = expectedBytes;
        lastComplete = complete;
        if (auto it = m_rows.find(assetId); it != m_rows.end()) {
            it->second.cachedSizeBytes = cachedBytes;
            it->second.complete = complete;
        }
    }
    void setLastError(const QString& assetId,
        const QString& error) override
    {
        ++errorCalls;
        lastErrorAssetId = assetId;
        lastError = error;
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

    void seedRow(const QString& assetId, domain::DownloadState state)
    {
        domain::DownloadItem it;
        it.assetId = assetId;
        it.state = state;
        m_rows[assetId] = it;
    }

    int stateCalls = 0;
    int cachedCalls = 0;
    int errorCalls = 0;
    QString lastStateAssetId;
    domain::DownloadState lastState = domain::DownloadState::Queued;
    QString lastCachedAssetId;
    qint64 lastCachedBytes = 0;
    std::optional<qint64> lastExpectedBytes;
    bool lastComplete = false;
    QString lastErrorAssetId;
    QString lastError;

private:
    std::map<QString, domain::DownloadItem> m_rows;
};

class EventSink : public QObject
{
    Q_OBJECT
public:
    explicit EventSink(PlaybackEventStream& stream, QObject* parent = nullptr)
        : QObject(parent)
    {
        connect(&stream, &PlaybackEventStream::eventPublished, this,
            [this](const PlaybackEvent& e) { received.push_back(e); });
    }
    std::vector<PlaybackEvent> received;
};

std::unique_ptr<TransferSession> makeSession(const QString& assetId,
    qint64 fileSize, FakeAssetSession** outSrc = nullptr)
{
    auto src = std::make_unique<FakeAssetSession>(assetId,
        QStringLiteral("file.mkv"), fileSize);
    if (outSrc) {
        *outSrc = src.get();
    }
    domain::AssetRef ref;
    ref.infoHash = QStringLiteral("hash");
    ref.releaseName = QStringLiteral("rel");
    return std::make_unique<TransferSession>(ref,
        domain::PlaybackContext {}, domain::DownloadBackendKind::Torrent,
        domain::DownloadMode::OnDemand,
        domain::CacheDisposition::Ephemeral, std::move(src));
}

} // namespace

class TestTransferSupervisor : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void cachedBytesWritesAndPublishesProgress();
    void cachedBytesPromotesIdleRowToActive();
    void completedWritesCompletedState();
    void failedSetsLastErrorAndPublishesFailed();
    void liveStatsPublishesStatsChanged();
    void sessionIdResolverIsHonored();
    void itemChangedFiresForEveryProjection();
};

void TestTransferSupervisor::cachedBytesWritesAndPublishesProgress()
{
    PlaybackEventStream stream;
    EventSink sink(stream);
    FakeDownloadRepo repo;
    SessionRegistry reg;
    TransferSupervisor sup(reg, repo, stream);

    FakeAssetSession* src = nullptr;
    auto session = makeSession(QStringLiteral("aid-1"), 2'000, &src);
    const auto assetId = session->assetId();
    reg.registerSession(std::move(session));

    src->emitCachedBytes(512);

    QCOMPARE(repo.cachedCalls, 1);
    QCOMPARE(repo.lastCachedAssetId, assetId);
    QCOMPARE(repo.lastCachedBytes, qint64(512));
    QCOMPARE(repo.lastExpectedBytes.value_or(-1), qint64(2'000));
    QCOMPARE(repo.lastComplete, false);

    QCOMPARE(sink.received.size(), std::size_t(1));
    const auto* prog = std::get_if<TransferProgressed>(&sink.received[0]);
    QVERIFY(prog);
    QCOMPARE(prog->assetId, assetId);
    QCOMPARE(prog->cachedBytes, qint64(512));
    QCOMPARE(prog->expectedBytes, qint64(2'000));
}

void TestTransferSupervisor::cachedBytesPromotesIdleRowToActive()
{
    PlaybackEventStream stream;
    EventSink sink(stream);
    FakeDownloadRepo repo;
    SessionRegistry reg;
    TransferSupervisor sup(reg, repo, stream);

    FakeAssetSession* src = nullptr;
    auto session = makeSession(QStringLiteral("aid-2"), 1'000, &src);
    const auto assetId = session->assetId();
    repo.seedRow(assetId, domain::DownloadState::Idle);
    reg.registerSession(std::move(session));

    src->emitCachedBytes(100);

    QCOMPARE(repo.stateCalls, 1);
    QCOMPARE(repo.lastStateAssetId, assetId);
    QCOMPARE(repo.lastState, domain::DownloadState::Active);
}

void TestTransferSupervisor::completedWritesCompletedState()
{
    PlaybackEventStream stream;
    EventSink sink(stream);
    FakeDownloadRepo repo;
    SessionRegistry reg;
    TransferSupervisor sup(reg, repo, stream);

    FakeAssetSession* src = nullptr;
    auto session = makeSession(QStringLiteral("aid-3"), 4'000, &src);
    const auto assetId = session->assetId();
    reg.registerSession(std::move(session));

    src->emitCompleted();

    QCOMPARE(repo.cachedCalls, 1);
    QCOMPARE(repo.lastCachedBytes, qint64(4'000));
    QCOMPARE(repo.lastComplete, true);
    QCOMPARE(repo.lastState, domain::DownloadState::Completed);

    bool sawCompleted = false;
    for (const auto& e : sink.received) {
        if (auto* tc = std::get_if<TransferCompleted>(&e); tc) {
            QCOMPARE(tc->assetId, assetId);
            sawCompleted = true;
        }
    }
    QVERIFY(sawCompleted);
}

void TestTransferSupervisor::failedSetsLastErrorAndPublishesFailed()
{
    PlaybackEventStream stream;
    EventSink sink(stream);
    FakeDownloadRepo repo;
    SessionRegistry reg;
    TransferSupervisor sup(reg, repo, stream);

    FakeAssetSession* src = nullptr;
    auto session = makeSession(QStringLiteral("aid-4"), 1, &src);
    const auto assetId = session->assetId();
    reg.registerSession(std::move(session));

    src->emitFailed(QStringLiteral("nope"));

    QCOMPARE(repo.errorCalls, 1);
    QCOMPARE(repo.lastErrorAssetId, assetId);
    QCOMPARE(repo.lastError, QStringLiteral("nope"));

    bool sawFailed = false;
    for (const auto& e : sink.received) {
        if (auto* tf = std::get_if<TransferFailed>(&e); tf) {
            QCOMPARE(tf->assetId, assetId);
            QCOMPARE(tf->reason, QStringLiteral("nope"));
            sawFailed = true;
        }
    }
    QVERIFY(sawFailed);
}

void TestTransferSupervisor::liveStatsPublishesStatsChanged()
{
    PlaybackEventStream stream;
    EventSink sink(stream);
    FakeDownloadRepo repo;
    SessionRegistry reg;
    TransferSupervisor sup(reg, repo, stream);

    FakeAssetSession* src = nullptr;
    auto session = makeSession(QStringLiteral("aid-5"), 1, &src);
    const auto assetId = session->assetId();
    reg.registerSession(std::move(session));

    src->emitLiveStats(800, 4, 2, 60);

    // No repo writes for live stats.
    QCOMPARE(repo.cachedCalls, 0);
    QCOMPARE(repo.stateCalls, 0);
    QCOMPARE(repo.errorCalls, 0);

    bool sawStats = false;
    for (const auto& e : sink.received) {
        if (auto* ts = std::get_if<TransferStatsChanged>(&e); ts) {
            QCOMPARE(ts->assetId, assetId);
            QCOMPARE(ts->bytesPerSec, qint64(800));
            QCOMPARE(ts->peers, 4);
            QCOMPARE(ts->seeds, 2);
            sawStats = true;
        }
    }
    QVERIFY(sawStats);
}

void TestTransferSupervisor::sessionIdResolverIsHonored()
{
    PlaybackEventStream stream;
    EventSink sink(stream);
    FakeDownloadRepo repo;
    SessionRegistry reg;
    TransferSupervisor sup(reg, repo, stream);

    const auto pid = QUuid::createUuid();
    sup.setSessionIdResolver([pid](const QString&) { return pid; });

    FakeAssetSession* src = nullptr;
    auto session = makeSession(QStringLiteral("aid-6"), 10, &src);
    reg.registerSession(std::move(session));
    src->emitCachedBytes(5);

    const auto* prog = std::get_if<TransferProgressed>(&sink.received[0]);
    QVERIFY(prog);
    QCOMPARE(prog->sessionId, pid);
}

void TestTransferSupervisor::itemChangedFiresForEveryProjection()
{
    PlaybackEventStream stream;
    EventSink sink(stream);
    FakeDownloadRepo repo;
    SessionRegistry reg;
    TransferSupervisor sup(reg, repo, stream);

    FakeAssetSession* src = nullptr;
    auto session = makeSession(QStringLiteral("aid-7"), 2'000, &src);
    reg.registerSession(std::move(session));

    QSignalSpy changed(&sup, &TransferSupervisor::itemChanged);
    src->emitCachedBytes(100);
    src->emitCompleted();
    src->emitFailed(QStringLiteral("e"));

    // cachedBytes + completed + failed => three itemChanged emits.
    // (liveStats does NOT emit itemChanged; that's intentional.)
    QCOMPARE(changed.size(), 3);
}

QTEST_MAIN(TestTransferSupervisor)
#include "tst_transfer_supervisor.moc"
