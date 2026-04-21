// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TorrentioClient.h"

#include "TestDoubles.h"

#include <QTest>

using kinema::api::MediaKind;
using kinema::api::TorrentioClient;
using kinema::core::torrentio::ConfigOptions;
using kinema::tests::FakeHttpClient;
using kinema::tests::loadJsonFixture;

class TstTorrentioClient : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testDefaultConfigPathIsIncluded()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("torrentio_stream_tt0133093.json") };

        TorrentioClient client(&http);
        const auto streams = QCoro::waitFor(client.streams(
            MediaKind::Movie, QStringLiteral("tt0133093")));

        QVERIFY(!streams.isEmpty());
        QCOMPARE(http.calls.size(), 1);
        QVERIFY(!http.calls.first().usedRequest);
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/sort=seeders/stream/movie/tt0133093.json"));
    }

    void testPopulatedConfigPathIncludesAllSegments()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("torrentio_stream_tt0133093.json") };

        TorrentioClient client(&http);
        ConfigOptions opts;
        opts.excludedResolutions = { QStringLiteral("4k") };
        opts.excludedCategories = { QStringLiteral("cam") };
        opts.providers = { QStringLiteral("yts") };
        opts.realDebridToken = QStringLiteral("RDTOKEN");

        const auto streams = QCoro::waitFor(client.streams(
            MediaKind::Series, QStringLiteral("tt0903747:1:1"), opts));

        QVERIFY(!streams.isEmpty());
        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/sort=seeders|qualityfilter=4k,cam|providers=yts|realdebrid=RDTOKEN/stream/series/tt0903747:1:1.json"));
    }
};

QTEST_MAIN(TstTorrentioClient)
#include "tst_torrentio_client.moc"
