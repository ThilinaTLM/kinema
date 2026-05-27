// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "TestDoubles.h"
#include "api/AllDebridClient.h"
#include "api/RealDebridClient.h"
#include "config/DownloadSettings.h"
#include "core/io/CachePaths.h"
#include "core/io/HttpClient.h"
#include "core/persistence/MediaCache.h"
#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/sources/DebridResolver.h"
#include "playback/ports/MediaSourcePort.h"
#include "playback/sources/AllDebridMediaSource.h"
#include "playback/sources/HttpRangeAssetSession.h"
#include "playback/sources/RealDebridMediaSource.h"

#include <KSharedConfig>

#include <QCoroSignal>
#include <QCoroTask>

#include <QDir>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>
#include <QUrl>

#include <memory>

using namespace kinema;
using kinema::tests::FakeHttpClient;

namespace {

class StubResolver final : public playback::sources::DebridResolver
{
public:
    playback::sources::ResolvedDebridLink reply;
    int resolveCalls = 0;

    QCoro::Task<playback::sources::ResolvedDebridLink> resolve(
        domain::AssetRef ref) override
    {
        Q_UNUSED(ref);
        ++resolveCalls;
        co_return reply;
    }
    QCoro::Task<void> cleanup(QString) override { co_return; }
};

domain::Stream makeStream()
{
    domain::Stream s;
    s.infoHash = QStringLiteral(
        "0123456789abcdef0123456789abcdef01234567");
    s.releaseName = QStringLiteral("Sample.Movie.2024");
    return s;
}

domain::AssetRef makeRef(const domain::Stream& s)
{
    domain::AssetRef ref;
    ref.infoHash = s.infoHash;
    ref.releaseName = s.releaseName;
    ref.fileIndex = 0;
    ref.fileNameHint = QStringLiteral("movie.mp4");
    return ref;
}

} // namespace

class TstDebridMediaSource : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
        m_config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-debrid-mediasource-test"),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<config::DownloadSettings>(m_config);
    }

    void init()
    {
        QDir(core::cache::mediaDir()).removeRecursively();
        QDir().mkpath(core::cache::mediaDir().absolutePath());

        m_cache = std::make_unique<core::MediaCache>(*m_settings);
        m_http = std::make_unique<FakeHttpClient>();
        m_rd = std::make_unique<api::RealDebridClient>(m_http.get());
        m_resolver = std::make_unique<StubResolver>();
        m_source = std::make_unique<
            playback::sources::RealDebridMediaSource>(*m_http, *m_rd,
            *m_resolver, *m_cache, *m_settings);
    }

    void cleanup()
    {
        m_source.reset();
        m_resolver.reset();
        m_rd.reset();
        m_http.reset();
        m_cache.reset();
    }

    void cleanupTestCase() { m_settings.reset(); }

    void kindIsRealDebrid()
    {
        QCOMPARE(m_source->kind(),
            domain::DownloadBackendKind::RealDebridHttp);
    }

    void canHandleRequiresTokenAndActionableId()
    {
        // No token -> never can handle.
        m_rd->setToken(QString());
        QVERIFY(!m_source->canHandle(makeStream()));

        m_rd->setToken(QStringLiteral("rd-token"));
        // With token: needs infoHash OR directUrl.
        domain::Stream empty;
        QVERIFY(!m_source->canHandle(empty));

        domain::Stream s = makeStream();
        QVERIFY(m_source->canHandle(s));

        domain::Stream onlyDirect;
        onlyDirect.directUrl
            = QUrl(QStringLiteral("https://host/file.mp4"));
        QVERIFY(m_source->canHandle(onlyDirect));
    }

    void openCallsResolverAndSetsModeAndFileSize()
    {
        m_rd->setToken(QStringLiteral("rd-token"));
        m_resolver->reply.downloadUrl
            = QUrl(QStringLiteral("https://host/file.mp4"));
        m_resolver->reply.fileSize = 12'345'678LL;
        m_resolver->reply.fileName = QStringLiteral("movie.mp4");
        m_resolver->reply.providerTorrentId
            = QStringLiteral("rdtid-123");

        const auto s = makeStream();
        const auto ref = makeRef(s);
        auto task = m_source->open(ref, s, domain::PlaybackContext {},
            domain::DownloadMode::OnDemand);
        const auto opened = QCoro::waitFor(std::move(task));

        QVERIFY(opened.session != nullptr);
        QCOMPARE(opened.assetId, domain::assetIdFor(ref));
        QCOMPARE(opened.session->assetId(), domain::assetIdFor(ref));
        QCOMPARE(opened.session->fileSize(), qint64(12'345'678));
        QCOMPARE(m_resolver->resolveCalls, 1);

        auto* http = dynamic_cast<
            playback::sources::HttpRangeAssetSession*>(
            opened.session.get());
        QVERIFY(http);
        QCOMPARE(http->mode(), domain::DownloadMode::OnDemand);
    }

    void openFullSetsModeFull()
    {
        m_rd->setToken(QStringLiteral("rd-token"));
        m_resolver->reply.downloadUrl
            = QUrl(QStringLiteral("https://host/file.mp4"));
        m_resolver->reply.fileSize = 1'000;
        m_resolver->reply.fileName = QStringLiteral("movie.mp4");
        m_resolver->reply.providerTorrentId
            = QStringLiteral("rdtid-200");

        const auto s = makeStream();
        auto task = m_source->open(makeRef(s), s,
            domain::PlaybackContext {}, domain::DownloadMode::Full);
        const auto opened = QCoro::waitFor(std::move(task));

        auto* http = dynamic_cast<
            playback::sources::HttpRangeAssetSession*>(
            opened.session.get());
        QVERIFY(http);
        QCOMPARE(http->mode(), domain::DownloadMode::Full);
    }

    void changeModeFlipsSessionMode()
    {
        m_rd->setToken(QStringLiteral("rd-token"));
        m_resolver->reply.downloadUrl
            = QUrl(QStringLiteral("https://host/file.mp4"));
        m_resolver->reply.fileSize = 1'000;
        m_resolver->reply.fileName = QStringLiteral("movie.mp4");
        m_resolver->reply.providerTorrentId
            = QStringLiteral("rdtid-300");

        const auto s = makeStream();
        auto task = m_source->open(makeRef(s), s,
            domain::PlaybackContext {}, domain::DownloadMode::OnDemand);
        const auto opened = QCoro::waitFor(std::move(task));

        m_source->changeMode(*opened.session, domain::DownloadMode::Full);
        auto* http = dynamic_cast<
            playback::sources::HttpRangeAssetSession*>(
            opened.session.get());
        QVERIFY(http);
        QCOMPARE(http->mode(), domain::DownloadMode::Full);

        // No-op when the session is already in the requested mode.
        m_source->changeMode(*opened.session, domain::DownloadMode::Full);
        QCOMPARE(http->mode(), domain::DownloadMode::Full);
    }

    // --- AllDebrid ----------------------------------------------

    void adKindIsAllDebrid()
    {
        api::AllDebridClient ad(m_http.get());
        StubResolver resolver;
        playback::sources::AllDebridMediaSource source(*m_http, ad,
            resolver, *m_cache, *m_settings);
        QCOMPARE(source.kind(),
            domain::DownloadBackendKind::AllDebridHttp);
    }

    void adCanHandleRequiresApiKey()
    {
        api::AllDebridClient ad(m_http.get());
        StubResolver resolver;
        playback::sources::AllDebridMediaSource source(*m_http, ad,
            resolver, *m_cache, *m_settings);

        ad.setApiKey(QString());
        QVERIFY(!source.canHandle(makeStream()));

        ad.setApiKey(QStringLiteral("ad-api-key"));
        QVERIFY(source.canHandle(makeStream()));

        domain::Stream empty;
        QVERIFY(!source.canHandle(empty));
    }

    void adOpenCallsResolverAndSetsMode()
    {
        api::AllDebridClient ad(m_http.get());
        ad.setApiKey(QStringLiteral("ad-api-key"));
        StubResolver resolver;
        resolver.reply.downloadUrl
            = QUrl(QStringLiteral("https://ad-host/file.mp4"));
        resolver.reply.fileSize = 999'999;
        resolver.reply.fileName = QStringLiteral("movie.mp4");
        resolver.reply.providerTorrentId = QStringLiteral("42");
        playback::sources::AllDebridMediaSource source(*m_http, ad,
            resolver, *m_cache, *m_settings);

        const auto s = makeStream();
        auto task = source.open(makeRef(s), s,
            domain::PlaybackContext {}, domain::DownloadMode::Full);
        const auto opened = QCoro::waitFor(std::move(task));

        QCOMPARE(resolver.resolveCalls, 1);
        QVERIFY(opened.session);
        QCOMPARE(opened.session->fileSize(), qint64(999'999));
        auto* http = dynamic_cast<
            playback::sources::HttpRangeAssetSession*>(
            opened.session.get());
        QVERIFY(http);
        QCOMPARE(http->mode(), domain::DownloadMode::Full);
    }

private:
    KSharedConfigPtr m_config;
    std::unique_ptr<config::DownloadSettings> m_settings;
    std::unique_ptr<core::MediaCache> m_cache;
    std::unique_ptr<FakeHttpClient> m_http;
    std::unique_ptr<api::RealDebridClient> m_rd;
    std::unique_ptr<StubResolver> m_resolver;
    std::unique_ptr<playback::sources::RealDebridMediaSource> m_source;
};

QTEST_MAIN(TstDebridMediaSource)
#include "tst_debrid_media_source.moc"
