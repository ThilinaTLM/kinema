// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/PeerflixIndexer.h"

#include "TestDoubles.h"
#include "config/PeerflixSettings.h"
#include "domain/DebridCredentials.h"

#include <KConfig>
#include <KConfigGroup>
#include <KSharedConfig>

#include <QCoro/QCoroSignal>
#include <QCoro/QCoroTask>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

namespace {
struct FakeDebridCreds final : public domain::DebridCredentialsProvider {
    domain::ActiveDebrid value;
    domain::ActiveDebrid active() const override { return value; }
};
} // namespace

class TstPeerflixIndexer : public QObject
{
    Q_OBJECT

private:
    QTemporaryDir m_tmp;
    KSharedConfigPtr m_config;

    void resetConfig()
    {
        m_config = KSharedConfig::openConfig(
            m_tmp.filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
        m_config->group(QStringLiteral("Peerflix")).deleteGroup();
    }

private Q_SLOTS:
    void init() { resetConfig(); }

    void testKindAndDisplayName()
    {
        config::PeerflixSettings settings(m_config);
        tests::FakeHttpClient http;
        api::PeerflixIndexer indexer(&http, settings, nullptr);
        QCOMPARE(indexer.kind(), domain::IndexerKind::Peerflix);
        QCOMPARE(indexer.displayName(), QStringLiteral("Peerflix"));
    }

    void testDefaultBaseUrlAndMoviePath()
    {
        config::PeerflixSettings settings(m_config);
        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, nullptr);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("peerflix.mov"));
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/stream/movie/tt0133093.json"));
    }

    void testCustomBaseUrlIsHonoured()
    {
        config::PeerflixSettings settings(m_config);
        settings.setBaseUrl(QStringLiteral("https://peerflix.mirror.example"));

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, nullptr);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("peerflix.mirror.example"));
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/stream/movie/tt0133093.json"));
    }

    void testSeriesPathShape()
    {
        config::PeerflixSettings settings(m_config);
        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0903747_1_1.json"));

        api::PeerflixIndexer indexer(&http, settings, nullptr);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Series,
            QStringLiteral("tt0903747:1:1")));

        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/stream/series/tt0903747:1:1.json"));
    }

    void testParserExtractsStructuredFields()
    {
        config::PeerflixSettings settings(m_config);
        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, nullptr);
        const auto streams = QCoro::waitFor(indexer.streams(
            domain::MediaKind::Movie, QStringLiteral("tt0133093")));

        QCOMPARE(streams.size(), 3);

        // Row 0: structured fields preferred over emoji regex.
        const auto& first = streams.at(0);
        QCOMPARE(first.infoHash,
            QStringLiteral("4927c5658396e63e40bc9061bd805a7bb1e06966"));
        QVERIFY(first.seeders.has_value());
        QCOMPARE(*first.seeders, 14);
        QVERIFY(first.sizeBytes.has_value());
        QCOMPARE(*first.sizeBytes, qint64(60333553090LL));
        QCOMPARE(first.resolution, QStringLiteral("4k"));
        QCOMPARE(first.language, QStringLiteral("es"));
        QCOMPARE(first.sources.size(), 2);

        // Row 1: description without metadata line, no fileIdx,
        // structured fields still populate seeders/size/resolution.
        const auto& second = streams.at(1);
        QCOMPARE(second.language, QStringLiteral("en"));
        QCOMPARE(second.resolution, QStringLiteral("1080p"));
        QVERIFY(second.seeders.has_value());
        QCOMPARE(*second.seeders, 612);

        // Row 2: seed=0 still yields a zero value (not unset).
        const auto& third = streams.at(2);
        QVERIFY(third.seeders.has_value());
        QCOMPARE(*third.seeders, 0);
        QCOMPARE(third.language, QStringLiteral("es"));
    }

    void testNoDebridUsesZeroConfigHost()
    {
        config::PeerflixSettings settings(m_config);
        FakeDebridCreds creds;
        // provider = None → even with the resolver wired in the
        // indexer must stay on the zero-config IPFS mirror.
        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("peerflix.mov"));
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/stream/movie/tt0133093.json"));
    }

    void testRealDebridSwitchesToAddonHost()
    {
        config::PeerflixSettings settings(m_config);
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::RealDebrid,
            QStringLiteral("rd-token") };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("addon.peerflix.mov"));
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral(
                "/debridoptions=torrentlinks,nocatalog|realdebrid=rd-token"
                "/stream/movie/tt0133093.json"));
    }

    void testAllDebridSwitchesToAddonHost()
    {
        config::PeerflixSettings settings(m_config);
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::AllDebrid,
            QStringLiteral("ad-key") };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("addon.peerflix.mov"));
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral(
                "/debridoptions=torrentlinks,nocatalog|alldebrid=ad-key"
                "/stream/movie/tt0133093.json"));
    }

    void testCustomAddonBaseUrlIsHonoured()
    {
        config::PeerflixSettings settings(m_config);
        settings.setAddonBaseUrl(
            QStringLiteral("https://addon.peerflix.mirror.example"));
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::RealDebrid,
            QStringLiteral("rd-token") };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("addon.peerflix.mirror.example"));
        // baseUrl override left untouched.
        QCOMPARE(settings.baseUrl(),
            QStringLiteral("https://peerflix.mov"));
    }

    void testEmptyTokenStaysOnZeroConfigHost()
    {
        config::PeerflixSettings settings(m_config);
        FakeDebridCreds creds;
        // Provider radio is set but the credential never made it
        // into the keyring — indexer must fall back to no-debrid.
        creds.value = { domain::DebridProvider::RealDebrid, QString {} };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("peerflix_stream_tt0133093.json"));

        api::PeerflixIndexer indexer(&http, settings, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("peerflix.mov"));
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/stream/movie/tt0133093.json"));
    }
};

QTEST_GUILESS_MAIN(TstPeerflixIndexer)
#include "tst_peerflix_indexer.moc"
