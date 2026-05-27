// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/DownloadSettings.h"
#include "core/io/CachePaths.h"
#include "core/persistence/MediaCache.h"
#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "download/TorrentAssetSession.h"
#include "playback/ports/MediaSourcePort.h"
#include "playback/sources/TorrentMediaSource.h"
#include "torrent/TorrentStreamingService.h"

#include <KSharedConfig>

#include <QCoroSignal>
#include <QCoroTask>

#include <QDir>
#include <QStandardPaths>
#include <QTest>

using namespace kinema;

namespace {

class StubTorrentEngine final : public torrent::TorrentStreamingService
{
public:
    explicit StubTorrentEngine(QObject* parent = nullptr)
        : torrent::TorrentStreamingService(StubTag {}, parent)
    {
    }

    QCoro::Task<torrent::PreparedSession> prepareSession(
        const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        torrent::PrepareMode mode) override
    {
        Q_UNUSED(ctx);
        ++prepareCalls;
        lastMode = mode;
        lastInfoHash = stream.infoHash;
        torrent::PreparedSession ps;
        ps.token = QStringLiteral("tok-stub");
        ps.fileName = QStringLiteral("movie.mkv");
        ps.fileSize = 4'000'000'000LL;
        ps.infoHash = stream.infoHash;
        co_return ps;
    }

    void setKeepAlive(const QString& infoHash, bool on) override
    {
        keepAliveCalls.append(qMakePair(infoHash, on));
    }
    void promoteToFull(const QString& infoHash) override
    {
        promoteCalls.append(infoHash);
        keepAliveCalls.append(qMakePair(infoHash, true));
    }
    QVector<torrent::TorrentFileEntry> filesForInfoHash(
        const QString& infoHash) const override
    {
        const auto it = stubFiles.find(infoHash.toLower());
        if (it == stubFiles.end()) {
            return {};
        }
        return it.value();
    }

    int prepareCalls = 0;
    torrent::PrepareMode lastMode = torrent::PrepareMode::Streaming;
    QString lastInfoHash;
    QList<QPair<QString, bool>> keepAliveCalls;
    QList<QString> promoteCalls;
    QHash<QString, QVector<torrent::TorrentFileEntry>> stubFiles;
};

domain::Stream makeStream(const QString& infoHash
    = QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee"))
{
    domain::Stream s;
    s.infoHash = infoHash;
    s.releaseName = QStringLiteral("Sample.Movie.2023");
    return s;
}

domain::AssetRef makeRef(const domain::Stream& s, int fileIndex = -1)
{
    domain::AssetRef ref;
    ref.infoHash = s.infoHash;
    ref.releaseName = s.releaseName;
    ref.fileIndex = fileIndex;
    return ref;
}

} // namespace

class TstTorrentMediaSource : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        QDir(core::cache::mediaDir()).removeRecursively();
        QDir().mkpath(core::cache::mediaDir().absolutePath());
        m_config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-tms-test"), KConfig::SimpleConfig);
        m_settings = std::make_unique<config::DownloadSettings>(m_config);
        m_settings->setCacheBudgetGb(1);
        m_cache = std::make_unique<core::MediaCache>(*m_settings);
        m_engine = std::make_unique<StubTorrentEngine>();
        m_source = std::make_unique<playback::sources::TorrentMediaSource>(
            *m_engine, *m_cache);
    }

    void cleanup()
    {
        m_source.reset();
        m_engine.reset();
        m_cache.reset();
        m_settings.reset();
    }

    void kindIsTorrent()
    {
        QCOMPARE(m_source->kind(), domain::DownloadBackendKind::Torrent);
    }

    void canHandleRequiresInfoHash()
    {
        domain::Stream s;
        QVERIFY(!m_source->canHandle(s));
        s.infoHash = QStringLiteral("abc");
        QVERIFY(m_source->canHandle(s));
    }

    void openOnDemandUsesStreamingMode()
    {
        const auto s = makeStream();
        const auto ref = makeRef(s);
        const auto ctx = domain::PlaybackContext {};

        auto task = m_source->open(ref, s, ctx,
            domain::DownloadMode::OnDemand);
        const auto opened = QCoro::waitFor(std::move(task));

        QCOMPARE(m_engine->prepareCalls, 1);
        QCOMPARE(m_engine->lastMode, torrent::PrepareMode::Streaming);
        QCOMPARE(m_engine->lastInfoHash, s.infoHash);
        QVERIFY(opened.session != nullptr);
        QCOMPARE(opened.assetId, domain::assetIdFor(ref));
        QCOMPARE(opened.session->assetId(), domain::assetIdFor(ref));
        QCOMPARE(opened.session->fileName(),
            QStringLiteral("movie.mkv"));
        // OnDemand must not set keep-alive on the engine.
        QVERIFY(m_engine->keepAliveCalls.isEmpty());
    }

    void openFullUsesBackgroundModeAndKeepAlive()
    {
        const auto s = makeStream();
        const auto ref = makeRef(s);

        auto task = m_source->open(ref, s, domain::PlaybackContext {},
            domain::DownloadMode::Full);
        const auto opened = QCoro::waitFor(std::move(task));

        QCOMPARE(m_engine->lastMode, torrent::PrepareMode::Background);
        QCOMPARE(m_engine->keepAliveCalls.size(), 1);
        QCOMPARE(m_engine->keepAliveCalls.first().first, s.infoHash);
        QCOMPARE(m_engine->keepAliveCalls.first().second, true);

        auto* legacy = dynamic_cast<download::TorrentAssetSession*>(
            opened.session.get());
        QVERIFY(legacy);
        QCOMPARE(legacy->mode(), domain::DownloadMode::Full);
    }

    void changeModePromoteToFullCallsEngine()
    {
        const auto s = makeStream();
        auto task = m_source->open(makeRef(s), s,
            domain::PlaybackContext {}, domain::DownloadMode::OnDemand);
        const auto opened = QCoro::waitFor(std::move(task));
        QVERIFY(opened.session);

        m_source->changeMode(*opened.session, domain::DownloadMode::Full);

        QCOMPARE(m_engine->promoteCalls.size(), 1);
        QCOMPARE(m_engine->promoteCalls.first(), s.infoHash);
        auto* legacy = dynamic_cast<download::TorrentAssetSession*>(
            opened.session.get());
        QVERIFY(legacy);
        QCOMPARE(legacy->mode(), domain::DownloadMode::Full);
    }

    void changeModeDemoteClearsKeepAlive()
    {
        const auto s = makeStream();
        auto task = m_source->open(makeRef(s), s,
            domain::PlaybackContext {}, domain::DownloadMode::Full);
        const auto opened = QCoro::waitFor(std::move(task));

        m_engine->keepAliveCalls.clear();
        m_source->changeMode(*opened.session,
            domain::DownloadMode::OnDemand);

        QCOMPARE(m_engine->keepAliveCalls.size(), 1);
        QCOMPARE(m_engine->keepAliveCalls.first().first, s.infoHash);
        QCOMPARE(m_engine->keepAliveCalls.first().second, false);
    }

    void changeModeNoOpWhenAlreadyAtMode()
    {
        const auto s = makeStream();
        auto task = m_source->open(makeRef(s), s,
            domain::PlaybackContext {}, domain::DownloadMode::OnDemand);
        const auto opened = QCoro::waitFor(std::move(task));

        m_engine->keepAliveCalls.clear();
        m_engine->promoteCalls.clear();
        m_source->changeMode(*opened.session,
            domain::DownloadMode::OnDemand);

        QVERIFY(m_engine->keepAliveCalls.isEmpty());
        QVERIFY(m_engine->promoteCalls.isEmpty());
    }

    void filesForLiftsToMediaFileEntry()
    {
        const auto s = makeStream();
        torrent::TorrentFileEntry e1 { 0, QStringLiteral("S01E01.mkv"),
            500'000 };
        torrent::TorrentFileEntry e2 { 1, QStringLiteral("S01E02.mkv"),
            600'000 };
        m_engine->stubFiles[s.infoHash.toLower()]
            = QVector<torrent::TorrentFileEntry> { e1, e2 };

        auto task = m_source->open(makeRef(s), s,
            domain::PlaybackContext {}, domain::DownloadMode::OnDemand);
        const auto opened = QCoro::waitFor(std::move(task));

        const auto files = m_source->filesFor(*opened.session);
        QCOMPARE(files.size(), 2);
        QCOMPARE(files.at(0).index, 0);
        QCOMPARE(files.at(0).path, QStringLiteral("S01E01.mkv"));
        QCOMPARE(files.at(0).size, qint64(500'000));
        QVERIFY(files.at(0).playable);
        QCOMPARE(files.at(1).index, 1);
    }

private:
    KSharedConfigPtr m_config;
    std::unique_ptr<config::DownloadSettings> m_settings;
    std::unique_ptr<core::MediaCache> m_cache;
    std::unique_ptr<StubTorrentEngine> m_engine;
    std::unique_ptr<playback::sources::TorrentMediaSource> m_source;
};

QTEST_MAIN(TstTorrentMediaSource)
#include "tst_torrent_media_source.moc"
