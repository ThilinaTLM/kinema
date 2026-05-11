// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/MediaFusionIndexer.h"

#include "TestDoubles.h"
#include "config/MediaFusionSettings.h"
#include "core/HttpError.h"

#include <KConfig>
#include <KConfigGroup>
#include <KSharedConfig>

#include <QCoro/QCoroSignal>
#include <QCoro/QCoroTask>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

class TstMediaFusionIndexer : public QObject
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
        m_config->group(QStringLiteral("MediaFusion")).deleteGroup();
    }

private Q_SLOTS:
    void init() { resetConfig(); }

    void testKindAndDisplayName()
    {
        config::MediaFusionSettings settings(m_config);
        tests::FakeHttpClient http;
        api::MediaFusionIndexer indexer(&http, settings);
        QCOMPARE(indexer.kind(), api::IndexerKind::MediaFusion);
        QCOMPARE(indexer.displayName(), QStringLiteral("MediaFusion"));
    }

    void testUnconfiguredThrowsBeforeHittingNetwork()
    {
        // No manifest URL saved → MediaFusion's public host refuses
        // unconfigured calls with HTTP 403, so the indexer short-
        // circuits with an actionable HttpError instead of letting
        // the user discover the problem one detail page at a time.
        config::MediaFusionSettings settings(m_config);
        tests::FakeHttpClient http;
        api::MediaFusionIndexer indexer(&http, settings);

        bool threw = false;
        try {
            (void)QCoro::waitFor(indexer.streams(
                api::MediaKind::Movie, QStringLiteral("tt0133093")));
        } catch (const core::HttpError& e) {
            threw = true;
            QVERIFY(e.message().contains(
                QStringLiteral("MediaFusion isn\u2019t configured")));
        }
        QVERIFY(threw);
        QCOMPARE(http.calls.size(), 0);
    }

    void testConfiguredHostThreadsTokenSegment()
    {
        config::MediaFusionSettings settings(m_config);
        QVERIFY(settings.saveFromManifestUrl(QStringLiteral(
            "https://mf.example/D-tok/manifest.json")));

        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("mediafusion_stream_tt0133093.json"));

        api::MediaFusionIndexer indexer(&http, settings);
        (void)QCoro::waitFor(indexer.streams(api::MediaKind::Series,
            QStringLiteral("tt0903747:1:1")));

        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().url.host(),
            QStringLiteral("mf.example"));
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/D-tok/stream/series/tt0903747:1:1.json"));
    }

    void testParserSharedWithTorrentio()
    {
        // MediaFusion uses the same emoji metadata convention as
        // Torrentio so the shared `stremio::parseStreams` handles
        // both. The 3rd row in the fixture has a `url` (RD-resolved)
        // and no seeders \u2014 same as the equivalent torrentio row.
        config::MediaFusionSettings settings(m_config);
        QVERIFY(settings.saveFromManifestUrl(QStringLiteral(
            "https://mf.example/D-tok/manifest.json")));
        tests::FakeHttpClient http;
        http.jsonReplies.append(
            tests::loadJsonFixture("mediafusion_stream_tt0133093.json"));

        api::MediaFusionIndexer indexer(&http, settings);
        const auto streams = QCoro::waitFor(indexer.streams(
            api::MediaKind::Movie, QStringLiteral("tt0133093")));

        const auto& rdRow = streams.at(2);
        QCOMPARE(rdRow.directUrl,
            QUrl(QStringLiteral("https://real-debrid.com/d/MF123/matrix.mkv")));
        QVERIFY(!rdRow.seeders.has_value());
    }
};

QTEST_GUILESS_MAIN(TstMediaFusionIndexer)
#include "tst_mediafusion_indexer.moc"
