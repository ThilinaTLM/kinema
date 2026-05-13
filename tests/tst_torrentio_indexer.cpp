// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TorrentioIndexer.h"

#include "TestDoubles.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "core/util/TorrentioConfig.h"
#include "domain/DebridCredentials.h"

#include <KConfig>
#include <KConfigGroup>
#include <KSharedConfig>

#include <QCoro/QCoroTask>
#include <QCoro/QCoroSignal>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

namespace {
/// Minimal `DebridCredentialsProvider` for indexer tests. Returns a
/// fixed snapshot — no keyring, no resolver, no signals.
struct FakeDebridCreds final : public domain::DebridCredentialsProvider {
    domain::ActiveDebrid value;
    domain::ActiveDebrid active() const override { return value; }
};
} // namespace

class TstTorrentioIndexer : public QObject
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
        // Wipe so each test starts clean.
        m_config->group(QStringLiteral("Torrentio")).deleteGroup();
        m_config->group(QStringLiteral("Filters")).deleteGroup();
    }

private Q_SLOTS:
    void init() { resetConfig(); }

    void testDefaultConfigPathIsIncluded()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_tt0133093.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, nullptr);

        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.size(), 1);
        // Default sort + no filter exclusions \u2192 path = "/sort=seeders/stream/movie/tt0133093.json"
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/sort=seeders/stream/movie/tt0133093.json"));
        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("torrentio.strem.fun"));
    }

    void testPopulatedConfigPathIncludesAllSegments()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        settings.setDefaultSort(core::torrentio::SortMode::Seeders);
        filter.setExcludedResolutions({ QStringLiteral("4k") });
        filter.setExcludedCategories({ QStringLiteral("cam") });

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_tt0133093.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, nullptr);

        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Series,
            QStringLiteral("tt0903747:1:1")));

        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral(
                "/sort=seeders|qualityfilter=4k,cam/stream/series/tt0903747:1:1.json"));
    }

    void testCustomBaseUrlIsHonoured()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        settings.setBaseUrl(QStringLiteral("https://torrentio.mirror.example"));

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_tt0133093.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, nullptr);

        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("torrentio.mirror.example"));
    }

    void testKindAndDisplayName()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        tests::FakeHttpClient http;
        api::TorrentioIndexer indexer(&http, settings, filter, nullptr);
        QCOMPARE(indexer.kind(), domain::IndexerKind::Torrentio);
        QCOMPARE(indexer.displayName(), QStringLiteral("Torrentio"));
    }

    void testRealDebridAppendedToConfigPath()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::RealDebrid,
            QStringLiteral("rd-token") };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_tt0133093.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral(
                "/sort=seeders|realdebrid=rd-token/stream/movie/tt0133093.json"));
    }

    void testAllDebridAppendedToConfigPath()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::AllDebrid,
            QStringLiteral("ad-key") };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_tt0133093.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral(
                "/sort=seeders|alldebrid=ad-key/stream/movie/tt0133093.json"));
    }

    void testEmptyTokenWithProviderFallsBackToNoDebrid()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        // Token is empty — the resolver normally collapses this to
        // `{None, {}}`, but we verify the indexer's own guard too:
        // even if `provider` looks set, the empty token must drop
        // the segment rather than emitting `realdebrid=`.
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::RealDebrid, QString {} };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_tt0133093.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/sort=seeders/stream/movie/tt0133093.json"));
    }

    void testRecoversInfoHashFromDebridResolveUrl()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::AllDebrid,
            QStringLiteral("AAAAAAAAAAAAAAAAAAAA") };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_alldebrid.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, &creds);
        const auto streams = QCoro::waitFor(indexer.streams(
            domain::MediaKind::Series,
            QStringLiteral("tt7587890:2:1")));

        QCOMPARE(streams.size(), 3);

        // [AD+] cached row: hash + file index + filename recovered
        // from the resolve URL; directUrl cleared so the credential
        // never leaves the process.
        const auto& cached = streams.at(0);
        QCOMPARE(cached.infoHash,
            QStringLiteral("255e2890a1a724bcdd93b1120dd08a5a8e11b949"));
        QCOMPARE(cached.fileIndex, 20);
        QCOMPARE(cached.fileNameHint,
            QStringLiteral("The Rookie S02E01.mkv"));
        QVERIFY(cached.directUrl.isEmpty());

        // [AD download] uncached row: same recovery path.
        const auto& pending = streams.at(1);
        QCOMPARE(pending.infoHash,
            QStringLiteral("bbccddeeff0011223344556677889900aabbccdd"));
        QCOMPARE(pending.fileIndex, 0);
        QCOMPARE(pending.fileNameHint,
            QStringLiteral("The Rookie S02E01.720p.mkv"));
        QVERIFY(pending.directUrl.isEmpty());

        // Raw torrent fallback row: untouched. The helper must not
        // damage rows that already carried their own structured data.
        const auto& raw = streams.at(2);
        QCOMPARE(raw.infoHash,
            QStringLiteral("99aabbccddeeff00112233445566778899aabbcc"));
        QCOMPARE(raw.fileIndex, 3);
        QCOMPARE(raw.fileNameHint,
            QStringLiteral("The.Rookie.S02E01.2160p.mkv"));
        QVERIFY(raw.directUrl.isEmpty());
    }

    void testDebridSegmentComesAfterQualityFilter()
    {
        config::TorrentioSettings settings(m_config);
        config::FilterSettings filter(m_config);
        filter.setExcludedResolutions({ QStringLiteral("4k") });
        FakeDebridCreds creds;
        creds.value = { domain::DebridProvider::RealDebrid,
            QStringLiteral("rd-token") };

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("torrentio_stream_tt0133093.json"));

        api::TorrentioIndexer indexer(&http, settings, filter, &creds);
        (void)QCoro::waitFor(indexer.streams(domain::MediaKind::Movie,
            QStringLiteral("tt0133093")));

        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral(
                "/sort=seeders|qualityfilter=4k|realdebrid=rd-token"
                "/stream/movie/tt0133093.json"));
    }
};

QTEST_GUILESS_MAIN(TstTorrentioIndexer)
#include "tst_torrentio_indexer.moc"
