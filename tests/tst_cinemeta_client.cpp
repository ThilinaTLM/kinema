// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/CinemetaClient.h"

#include "TestDoubles.h"

#include <QTest>

using kinema::api::CinemetaClient;
using kinema::api::MediaKind;
using kinema::tests::FakeHttpClient;
using kinema::tests::loadJsonFixture;

class TstCinemetaClient : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testSearchEncodesQueryIntoPath()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("cinemeta_search_matrix.json") };

        CinemetaClient client(&http);
        const auto results = QCoro::waitFor(
            client.search(MediaKind::Movie, QStringLiteral("the matrix")));

        QCOMPARE(results.size(), 2);
        QCOMPARE(http.calls.size(), 1);
        QVERIFY(http.calls.first().json);
        QVERIFY(!http.calls.first().usedRequest);
        QCOMPARE(http.calls.first().url.path(),
            QStringLiteral("/catalog/movie/top/search=the%20matrix.json"));
    }

    void testMetaEndpointsUseExpectedPaths()
    {
        FakeHttpClient http;
        http.jsonReplies = {
            loadJsonFixture("cinemeta_meta_tt0133093.json"),
            loadJsonFixture("cinemeta_meta_tt0903747_breaking_bad.json")
        };

        CinemetaClient client(&http);
        const auto movie = QCoro::waitFor(
            client.meta(MediaKind::Movie, QStringLiteral("tt0133093")));
        const auto series = QCoro::waitFor(
            client.seriesMeta(QStringLiteral("tt0903747")));

        QCOMPARE(movie.summary.imdbId, QStringLiteral("tt0133093"));
        QCOMPARE(series.meta.summary.imdbId, QStringLiteral("tt0903747"));
        QCOMPARE(http.calls.size(), 2);
        QCOMPARE(http.calls.at(0).url.path(),
            QStringLiteral("/meta/movie/tt0133093.json"));
        QCOMPARE(http.calls.at(1).url.path(),
            QStringLiteral("/meta/series/tt0903747.json"));
    }
};

QTEST_MAIN(TstCinemetaClient)
#include "tst_cinemeta_client.moc"
