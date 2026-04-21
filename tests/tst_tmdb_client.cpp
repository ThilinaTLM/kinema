// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TmdbClient.h"

#include "core/HttpError.h"
#include "TestDoubles.h"

#include <QTest>
#include <QUrlQuery>

using kinema::api::DiscoverQuery;
using kinema::api::DiscoverSort;
using kinema::api::MediaKind;
using kinema::api::TmdbClient;
using kinema::core::HttpError;
using kinema::tests::FakeHttpClient;
using kinema::tests::loadJsonFixture;

class TstTmdbClient : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testTrendingAddsAuthAndLanguageHeader()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("tmdb_trending_week_movie.json") };

        TmdbClient client(&http);
        client.setToken(QStringLiteral("tmdb-token"));
        client.setLanguage(QStringLiteral("en-US"));

        const auto items = QCoro::waitFor(client.trending(MediaKind::Movie, true));

        QCOMPARE(items.size(), 3);
        QCOMPARE(http.calls.size(), 1);
        QVERIFY(http.calls.first().usedRequest);
        QCOMPARE(http.calls.first().request.rawHeader("Authorization"),
            QByteArrayLiteral("Bearer tmdb-token"));
        QCOMPARE(http.calls.first().request.rawHeader("Accept"),
            QByteArrayLiteral("application/json"));
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/3/trending/movie/week"));
        const QUrlQuery query(http.calls.first().request.url());
        QCOMPARE(query.queryItemValue(QStringLiteral("language")),
            QStringLiteral("en-US"));
    }

    void testDiscoverBuildsExpectedUrl()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("tmdb_discover_movie_page1.json") };

        TmdbClient client(&http);
        client.setToken(QStringLiteral("tmdb-token"));
        client.setLanguage(QStringLiteral("en-US"));
        client.setRegion(QStringLiteral("US"));

        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.withGenreIds = { 28, 12 };
        q.voteAverageGte = 7.5;
        q.voteCountGte = 500;
        q.sort = DiscoverSort::Rating;
        q.page = 2;

        const auto page = QCoro::waitFor(client.discover(q));

        QCOMPARE(page.page, 1);
        QCOMPARE(http.calls.size(), 1);
        const auto& request = http.calls.first().request;
        QCOMPARE(request.url().path(), QStringLiteral("/3/discover/movie"));
        const QUrlQuery query(request.url());
        QCOMPARE(query.queryItemValue(QStringLiteral("language")),
            QStringLiteral("en-US"));
        QCOMPARE(query.queryItemValue(QStringLiteral("with_genres")),
            QStringLiteral("28,12"));
        QCOMPARE(query.queryItemValue(QStringLiteral("vote_average.gte")),
            QStringLiteral("7.5"));
        QCOMPARE(query.queryItemValue(QStringLiteral("vote_count.gte")),
            QStringLiteral("500"));
        QCOMPARE(query.queryItemValue(QStringLiteral("sort_by")),
            QStringLiteral("vote_average.desc"));
        QCOMPARE(query.queryItemValue(QStringLiteral("page")),
            QStringLiteral("2"));
    }

    void testNoTokenFailsBeforeRequest()
    {
        FakeHttpClient http;
        TmdbClient client(&http);

        try {
            (void)QCoro::waitFor(client.trending(MediaKind::Movie));
            QFAIL("expected HttpError");
        } catch (const HttpError& e) {
            QCOMPARE(e.httpStatus(), 401);
            QVERIFY(http.calls.isEmpty());
        }
    }
};

QTEST_MAIN(TstTmdbClient)
#include "tst_tmdb_client.moc"
