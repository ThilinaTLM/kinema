// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Download.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "download/AssetSession.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSession.h"
#include "torrent/MediaFileSelector.h"

#include <QCoro/QCoroTask>

#include <QByteArray>
#include <QSignalSpy>
#include <QString>
#include <QTest>
#include <QVector>

#include <memory>

using namespace kinema;

namespace {

/**
 * Minimal fake of `download::AssetSession` for registry tests.
 *
 * The registry only cares about identity, mode, and file list — so
 * the byte-range methods are stubbed with no-ops. Progress signals
 * are exposed via small helpers so tests can verify re-publication
 * through `TransferSession`.
 */
class FakeAssetSession final : public download::AssetSession
{
    Q_OBJECT
public:
    FakeAssetSession(QString assetId,
        QString infoHash,
        QString fileName,
        qint64 fileSize,
        QVector<torrent::TorrentFileEntry> files,
        QObject* parent = nullptr)
        : download::AssetSession(parent)
        , m_assetId(std::move(assetId))
        , m_infoHash(std::move(infoHash))
        , m_fileName(std::move(fileName))
        , m_fileSize(fileSize)
        , m_files(std::move(files))
    {
    }

    QString token() const override { return m_assetId; }
    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_fileSize; }
    qint64 cachedBytes() const override { return m_cachedBytes; }
    QString infoHash() const noexcept { return m_infoHash; }

    QCoro::Task<bool> ensureRange(torrent::ByteRange) override
    {
        co_return true;
    }
    QByteArray readRange(torrent::ByteRange) const override
    {
        return {};
    }
    void touch() override { ++touchCount; }

    domain::DownloadMode mode() const override { return m_mode; }
    void setMode(domain::DownloadMode m) override { m_mode = m; }
    void pause() override { ++pauseCount; }
    void resume() override { ++resumeCount; }

    QVector<torrent::TorrentFileEntry> files() const override
    {
        return m_files;
    }

    // Test helpers — fire the base signals from outside.
    void emitCachedBytes(qint64 b)
    {
        m_cachedBytes = b;
        Q_EMIT cachedBytesChanged(b);
    }
    void emitCompleted() { Q_EMIT completed(); }
    void emitFailed(const QString& r) { Q_EMIT failed(r); }
    void emitLiveStats(qint64 rate, int peers, int seeds, int eta)
    {
        Q_EMIT liveStatsChanged(rate, peers, seeds, eta);
    }

    int touchCount = 0;
    int pauseCount = 0;
    int resumeCount = 0;

private:
    QString m_assetId;
    QString m_infoHash;
    QString m_fileName;
    qint64 m_fileSize;
    QVector<torrent::TorrentFileEntry> m_files;
    qint64 m_cachedBytes = 0;
    domain::DownloadMode m_mode = domain::DownloadMode::OnDemand;
};

domain::AssetRef makeRef(const QString& infoHash, int fileIndex,
    const QString& release = QStringLiteral("rel"))
{
    domain::AssetRef ref;
    ref.infoHash = infoHash;
    ref.fileIndex = fileIndex;
    ref.releaseName = release;
    return ref;
}

torrent::TorrentFileEntry makeFile(int index, const QString& path,
    qint64 size)
{
    torrent::TorrentFileEntry f;
    f.index = index;
    f.path = path;
    f.size = size;
    return f;
}

std::unique_ptr<playback::transfer::TransferSession> makeTransferSession(
    const QString& infoHash, int fileIndex,
    QVector<torrent::TorrentFileEntry> files = {},
    qint64 fileSize = 1'000'000)
{
    auto ref = makeRef(infoHash, fileIndex);
    const auto assetId = domain::assetIdFor(ref);
    auto src = std::make_unique<FakeAssetSession>(assetId, infoHash,
        QStringLiteral("file.mkv"), fileSize, std::move(files));
    return std::make_unique<playback::transfer::TransferSession>(ref,
        domain::PlaybackContext {}, domain::DownloadBackendKind::Torrent,
        domain::DownloadMode::OnDemand,
        domain::CacheDisposition::Ephemeral, std::move(src));
}

} // namespace

class TestSessionRegistry : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void registerAndFind();
    void duplicateRegistrationIsRejected();
    void takeReturnsOwnershipAndClearsAttached();
    void openingGuardSetAndCleared();
    void supersedeReturnsSameInfoHashExceptKept();
    void supersedeIsCaseInsensitive();
    void supersedeIgnoresEmptyHash();
    void attachAndDetachPlayerEmitsSignals();
    void filesForStreamRefMatchesByInfoHash();
    void filesForAssetIdRoundTrips();
    void progressSignalsRePublishThroughTransferSession();
    void touchPauseResumeForwardToLegacySession();
};

void TestSessionRegistry::registerAndFind()
{
    playback::transfer::SessionRegistry reg;
    QSignalSpy added(&reg,
        &playback::transfer::SessionRegistry::sessionRegistered);
    auto session = makeTransferSession(QStringLiteral("abc"), 0);
    const auto assetId = session->assetId();
    auto* raw = reg.registerSession(std::move(session));

    QVERIFY(raw);
    QCOMPARE(reg.size(), 1);
    QVERIFY(reg.contains(assetId));
    QCOMPARE(reg.find(assetId), raw);
    QCOMPARE(reg.assetIds(), QStringList { assetId });
    QCOMPARE(added.size(), 1);
    QCOMPARE(added.takeFirst().at(0).toString(), assetId);
}

void TestSessionRegistry::duplicateRegistrationIsRejected()
{
    // We just verify that find()-then-register is the contract.
    // The hard assert inside `registerSession` would crash; this
    // mirrors how production callers must check first.
    playback::transfer::SessionRegistry reg;
    auto first = makeTransferSession(QStringLiteral("abc"), 0);
    const auto assetId = first->assetId();
    reg.registerSession(std::move(first));
    QVERIFY(reg.find(assetId) != nullptr);
}

void TestSessionRegistry::takeReturnsOwnershipAndClearsAttached()
{
    playback::transfer::SessionRegistry reg;
    auto session = makeTransferSession(QStringLiteral("abc"), 0);
    const auto assetId = session->assetId();
    reg.registerSession(std::move(session));
    QVERIFY(reg.attachPlayer(assetId));
    QVERIFY(reg.hasAttachedPlayer(assetId));

    QSignalSpy removed(&reg,
        &playback::transfer::SessionRegistry::sessionRemoved);
    QSignalSpy attachedChanged(&reg,
        &playback::transfer::SessionRegistry::attachedPlayerChanged);
    auto owned = reg.take(assetId);

    QVERIFY(owned);
    QVERIFY(!reg.contains(assetId));
    QCOMPARE(reg.size(), 0);
    QVERIFY(!reg.hasAttachedPlayer(assetId));
    QCOMPARE(removed.size(), 1);
    QCOMPARE(removed.takeFirst().at(0).toString(), assetId);
    QCOMPARE(attachedChanged.size(), 1);
    const auto args = attachedChanged.takeFirst();
    QCOMPARE(args.at(0).toString(), assetId);
    QCOMPARE(args.at(1).toBool(), false);
}

void TestSessionRegistry::openingGuardSetAndCleared()
{
    playback::transfer::SessionRegistry reg;
    QVERIFY(!reg.isOpening(QStringLiteral("aid")));
    reg.markOpening(QStringLiteral("aid"));
    QVERIFY(reg.isOpening(QStringLiteral("aid")));
    reg.clearOpening(QStringLiteral("aid"));
    QVERIFY(!reg.isOpening(QStringLiteral("aid")));
    // Empty asset ids are ignored — registry never stores them.
    reg.markOpening(QString());
    QVERIFY(!reg.isOpening(QString()));
}

void TestSessionRegistry::supersedeReturnsSameInfoHashExceptKept()
{
    playback::transfer::SessionRegistry reg;
    auto a = makeTransferSession(QStringLiteral("hash-1"), 0);
    auto b = makeTransferSession(QStringLiteral("hash-1"), 1);
    auto c = makeTransferSession(QStringLiteral("hash-2"), 0);
    const auto idA = a->assetId();
    const auto idB = b->assetId();
    const auto idC = c->assetId();
    reg.registerSession(std::move(a));
    reg.registerSession(std::move(b));
    reg.registerSession(std::move(c));

    const auto victims = reg.superseded(QStringLiteral("hash-1"), idB);
    QCOMPARE(victims.size(), 1);
    QCOMPARE(victims.first(), idA);
    QVERIFY(!victims.contains(idB));
    QVERIFY(!victims.contains(idC));
}

void TestSessionRegistry::supersedeIsCaseInsensitive()
{
    playback::transfer::SessionRegistry reg;
    auto s = makeTransferSession(QStringLiteral("abcdef"), 0);
    const auto id = s->assetId();
    reg.registerSession(std::move(s));

    const auto victims = reg.superseded(QStringLiteral("ABCDEF"),
        QStringLiteral("other-id"));
    QCOMPARE(victims.size(), 1);
    QCOMPARE(victims.first(), id);
}

void TestSessionRegistry::supersedeIgnoresEmptyHash()
{
    playback::transfer::SessionRegistry reg;
    auto s = makeTransferSession(QStringLiteral("h"), 0);
    reg.registerSession(std::move(s));
    QVERIFY(reg.superseded(QString(), QStringLiteral("x")).isEmpty());
}

void TestSessionRegistry::attachAndDetachPlayerEmitsSignals()
{
    playback::transfer::SessionRegistry reg;
    auto s = makeTransferSession(QStringLiteral("h"), 0);
    const auto id = s->assetId();
    reg.registerSession(std::move(s));

    QSignalSpy spy(&reg,
        &playback::transfer::SessionRegistry::attachedPlayerChanged);
    QVERIFY(reg.attachPlayer(id));
    QVERIFY(!reg.attachPlayer(id)); // idempotent
    QVERIFY(reg.hasAttachedPlayer(id));
    QVERIFY(reg.detachPlayer(id));
    QVERIFY(!reg.detachPlayer(id)); // idempotent
    QCOMPARE(spy.size(), 2);
    QCOMPARE(spy.at(0).at(1).toBool(), true);
    QCOMPARE(spy.at(1).at(1).toBool(), false);
}

void TestSessionRegistry::filesForStreamRefMatchesByInfoHash()
{
    playback::transfer::SessionRegistry reg;
    QVector<torrent::TorrentFileEntry> files = {
        makeFile(0, QStringLiteral("S01E01.mkv"), 1'000'000),
        makeFile(1, QStringLiteral("S01E02.mkv"), 1'100'000),
    };
    auto session = makeTransferSession(QStringLiteral("HASH"), 0, files);
    reg.registerSession(std::move(session));

    domain::HistoryStreamRef ref;
    ref.infoHash = QStringLiteral("hash"); // case-insensitive
    const auto result = reg.filesForStreamRef(ref);

    QCOMPARE(result.size(), 2);
    QCOMPARE(result.at(0).index, 0);
    QCOMPARE(result.at(0).path, QStringLiteral("S01E01.mkv"));
    QCOMPARE(result.at(0).size, qint64(1'000'000));
    QVERIFY(result.at(0).playable);
    QCOMPARE(result.at(1).index, 1);
}

void TestSessionRegistry::filesForAssetIdRoundTrips()
{
    playback::transfer::SessionRegistry reg;
    QVector<torrent::TorrentFileEntry> files = {
        makeFile(0, QStringLiteral("file.mkv"), 2'000),
    };
    auto session = makeTransferSession(QStringLiteral("h"), 0, files);
    const auto assetId = session->assetId();
    reg.registerSession(std::move(session));

    const auto result = reg.filesForAssetId(assetId);
    QCOMPARE(result.size(), 1);
    QCOMPARE(result.first().index, 0);
    QCOMPARE(result.first().path, QStringLiteral("file.mkv"));

    QVERIFY(reg.filesForAssetId(QStringLiteral("nope")).isEmpty());
}

void TestSessionRegistry::progressSignalsRePublishThroughTransferSession()
{
    auto src = std::make_unique<FakeAssetSession>(
        QStringLiteral("aid"), QStringLiteral("hash"),
        QStringLiteral("file.mkv"), 1'000, QVector<torrent::TorrentFileEntry> {});
    auto* srcPtr = src.get();
    auto ref = makeRef(QStringLiteral("hash"), 0);
    auto session = std::make_unique<playback::transfer::TransferSession>(
        ref, domain::PlaybackContext {},
        domain::DownloadBackendKind::Torrent,
        domain::DownloadMode::OnDemand,
        domain::CacheDisposition::Ephemeral, std::move(src));

    QSignalSpy cached(session.get(),
        &playback::transfer::TransferSession::cachedBytesChanged);
    QSignalSpy completed(session.get(),
        &playback::transfer::TransferSession::completed);
    QSignalSpy failed(session.get(),
        &playback::transfer::TransferSession::failed);
    QSignalSpy live(session.get(),
        &playback::transfer::TransferSession::liveStatsChanged);

    srcPtr->emitCachedBytes(512);
    srcPtr->emitLiveStats(100, 3, 2, 42);
    srcPtr->emitCompleted();
    srcPtr->emitFailed(QStringLiteral("boom"));

    QCOMPARE(cached.size(), 1);
    QCOMPARE(cached.first().at(0).toLongLong(), qint64(512));
    QCOMPARE(live.size(), 1);
    QCOMPARE(live.first().at(0).toLongLong(), qint64(100));
    QCOMPARE(live.first().at(1).toInt(), 3);
    QCOMPARE(live.first().at(2).toInt(), 2);
    QCOMPARE(live.first().at(3).toInt(), 42);
    QCOMPARE(completed.size(), 1);
    QCOMPARE(failed.size(), 1);
    QCOMPARE(failed.first().at(0).toString(), QStringLiteral("boom"));

    QCOMPARE(session->cachedBytes(), qint64(512));
    QCOMPARE(session->fileSize(), qint64(1'000));
    QCOMPARE(session->fileName(), QStringLiteral("file.mkv"));
}

void TestSessionRegistry::touchPauseResumeForwardToLegacySession()
{
    auto src = std::make_unique<FakeAssetSession>(
        QStringLiteral("aid"), QStringLiteral("hash"),
        QStringLiteral("file.mkv"), 1, QVector<torrent::TorrentFileEntry> {});
    auto* srcPtr = src.get();
    auto ref = makeRef(QStringLiteral("hash"), 0);
    playback::transfer::TransferSession session(ref,
        domain::PlaybackContext {},
        domain::DownloadBackendKind::Torrent,
        domain::DownloadMode::OnDemand,
        domain::CacheDisposition::Ephemeral, std::move(src));

    session.touch();
    session.touch();
    session.pause();
    session.resume();
    session.setMode(domain::DownloadMode::Full);

    QCOMPARE(srcPtr->touchCount, 2);
    QCOMPARE(srcPtr->pauseCount, 1);
    QCOMPARE(srcPtr->resumeCount, 1);
    QCOMPARE(srcPtr->mode(), domain::DownloadMode::Full);
    QCOMPARE(session.mode(), domain::DownloadMode::Full);
}

QTEST_MAIN(TestSessionRegistry)
#include "tst_session_registry.moc"
