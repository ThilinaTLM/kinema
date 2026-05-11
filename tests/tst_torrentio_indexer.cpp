// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TorrentioIndexer.h"

#include "TestDoubles.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "core/util/TorrentioConfig.h"

#include <KConfig>
#include <KConfigGroup>
#include <KSharedConfig>

#include <QCoro/QCoroTask>
#include <QCoro/QCoroSignal>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

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

        api::TorrentioIndexer indexer(&http, settings, filter);

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

        api::TorrentioIndexer indexer(&http, settings, filter);

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

        api::TorrentioIndexer indexer(&http, settings, filter);

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
        api::TorrentioIndexer indexer(&http, settings, filter);
        QCOMPARE(indexer.kind(), domain::IndexerKind::Torrentio);
        QCOMPARE(indexer.displayName(), QStringLiteral("Torrentio"));
    }
};

QTEST_GUILESS_MAIN(TstTorrentioIndexer)
#include "tst_torrentio_indexer.moc"
