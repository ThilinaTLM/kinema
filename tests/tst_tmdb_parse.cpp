// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TmdbParse.h"
#include "core/HttpError.h"

#include <QFile>
#include <QJsonDocument>
#include <QTest>

using namespace kinema::api;

class TstTmdbParse : public QObject
{
    Q_OBJECT
private:
    static QJsonDocument loadFixture(const char* name)
    {
        QFile f(QStringLiteral(KINEMA_TEST_FIXTURES_DIR) + QLatin1Char('/')
            + QString::fromLatin1(name));
        if (!f.open(QIODevice::ReadOnly)) {
            qFatal("Cannot open fixture %s", name);
        }
        return QJsonDocument::fromJson(f.readAll());
    }

private Q_SLOTS:
    void composeImageUrl_builds_full_cdn_url()
    {
        const auto u = tmdb::composeImageUrl(QStringLiteral("w342"),
            QStringLiteral("/abc.jpg"));
        QCOMPARE(u, QUrl(QStringLiteral(
            "https://image.tmdb.org/t/p/w342/abc.jpg")));
    }

    void composeImageUrl_empty_path_returns_empty()
    {
        QVERIFY(tmdb::composeImageUrl(QStringLiteral("w342"),
            QString {}).isEmpty());
    }

    void composeImageUrl_tolerates_missing_leading_slash()
    {
        const auto u = tmdb::composeImageUrl(QStringLiteral("w342"),
            QStringLiteral("abc.jpg"));
        QCOMPARE(u, QUrl(QStringLiteral(
            "https://image.tmdb.org/t/p/w342/abc.jpg")));
    }

    void parseList_movie_drops_missing_poster_and_bogus_rows()
    {
        const auto doc = loadFixture("tmdb_trending_week_movie.json");
        const auto rows = tmdb::parseList(doc, MediaKind::Movie);

        // Fixture has 5 rows: 2 valid movies, 1 null poster, 1 id=0,
        // and 1 with empty title but populated original_title (kept).
        QCOMPARE(rows.size(), 3);

        QCOMPARE(rows.at(0).tmdbId, 603);
        QCOMPARE(rows.at(0).kind, MediaKind::Movie);
        QCOMPARE(rows.at(0).title, QStringLiteral("The Matrix"));
        QVERIFY(rows.at(0).year.has_value());
        QCOMPARE(*rows.at(0).year, 1999);
        QVERIFY(rows.at(0).voteAverage.has_value());
        QCOMPARE(*rows.at(0).voteAverage, 8.7);
        QCOMPARE(rows.at(0).poster, QUrl(QStringLiteral(
            "https://image.tmdb.org/t/p/w342/matrix_poster.jpg")));
        QCOMPARE(rows.at(0).backdrop, QUrl(QStringLiteral(
            "https://image.tmdb.org/t/p/w780/trend_bd_1.jpg")));

        // The empty-title / fallback-to-original_title row:
        QCOMPARE(rows.at(2).tmdbId, 777);
        QCOMPARE(rows.at(2).title, QStringLiteral("Only Original Title"));
        // vote_average 0.0 → surfaced as absent.
        QVERIFY(!rows.at(2).voteAverage.has_value());
    }

    void parseList_tv_uses_name_and_first_air_date()
    {
        const auto doc = loadFixture("tmdb_popular_tv.json");
        const auto rows = tmdb::parseList(doc, MediaKind::Series);

        QCOMPARE(rows.size(), 2);
        QCOMPARE(rows.at(0).tmdbId, 1396);
        QCOMPARE(rows.at(0).kind, MediaKind::Series);
        QCOMPARE(rows.at(0).title, QStringLiteral("Breaking Bad"));
        QVERIFY(rows.at(0).year.has_value());
        QCOMPARE(*rows.at(0).year, 2008);
    }

    void parseList_throws_on_non_object()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("[]"));
        QVERIFY_EXCEPTION_THROWN(
            tmdb::parseList(doc, MediaKind::Movie),
            kinema::core::HttpError);
    }

    void parseList_empty_results_returns_empty_not_error()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        const auto rows = tmdb::parseList(doc, MediaKind::Movie);
        QVERIFY(rows.isEmpty());
    }

    void parseMovieExternalIds_returns_imdb_id_from_top_level()
    {
        const auto doc = loadFixture(
            "tmdb_movie_detail_with_external_ids.json");
        QCOMPARE(tmdb::parseMovieExternalIds(doc),
            QStringLiteral("tt0133093"));
    }

    void parseMovieExternalIds_falls_back_to_nested()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray(
            R"({"id":1,"external_ids":{"imdb_id":"tt1234567"}})"));
        QCOMPARE(tmdb::parseMovieExternalIds(doc),
            QStringLiteral("tt1234567"));
    }

    void parseMovieExternalIds_missing_returns_empty()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray(
            R"({"id":1})"));
        QVERIFY(tmdb::parseMovieExternalIds(doc).isEmpty());
    }

    void parseSeriesExternalIds_returns_imdb_id()
    {
        const auto doc = loadFixture("tmdb_series_external_ids.json");
        QCOMPARE(tmdb::parseSeriesExternalIds(doc),
            QStringLiteral("tt0903747"));
    }

    void parseFindResult_prefers_movie_when_requested_and_present()
    {
        const auto doc = loadFixture("tmdb_find_by_imdb.json");
        const auto [id, kind] = tmdb::parseFindResult(doc, MediaKind::Movie);
        QCOMPARE(id, 603);
        QCOMPARE(kind, MediaKind::Movie);
    }

    void parseFindResult_falls_back_to_tv_when_movie_empty()
    {
        const auto doc = loadFixture("tmdb_find_by_imdb_series.json");
        const auto [id, kind] = tmdb::parseFindResult(doc, MediaKind::Movie);
        QCOMPARE(id, 1396);
        QCOMPARE(kind, MediaKind::Series);
    }

    void parseFindResult_prefers_tv_when_requested()
    {
        const auto doc = loadFixture("tmdb_find_by_imdb_series.json");
        const auto [id, kind] = tmdb::parseFindResult(doc, MediaKind::Series);
        QCOMPARE(id, 1396);
        QCOMPARE(kind, MediaKind::Series);
    }

    void parseFindResult_returns_zero_when_empty()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray(
            R"({"movie_results":[],"tv_results":[]})"));
        const auto [id, kind] = tmdb::parseFindResult(doc, MediaKind::Movie);
        QCOMPARE(id, 0);
    }

    void parseRecommendations_shape_matches_parseList()
    {
        const auto doc = loadFixture("tmdb_recommendations_movie.json");
        const auto rows = tmdb::parseList(doc, MediaKind::Movie);
        QCOMPARE(rows.size(), 2);
        QCOMPARE(rows.at(0).tmdbId, 604);
        QCOMPARE(rows.at(0).title, QStringLiteral("The Matrix Reloaded"));
    }

    void parsePagedList_extracts_items_and_paging_metadata()
    {
        const auto doc = loadFixture("tmdb_discover_movie_page1.json");
        const auto r = tmdb::parsePagedList(doc, MediaKind::Movie);

        // 3 results in the fixture; the third is dropped for missing
        // poster_path, so we expect 2.
        QCOMPARE(r.items.size(), 2);
        QCOMPARE(r.items.at(0).tmdbId, 1000001);
        QCOMPARE(r.items.at(0).title, QStringLiteral("Alpha Protocol"));
        QVERIFY(r.items.at(0).year.has_value());
        QCOMPARE(*r.items.at(0).year, 2026);

        QCOMPARE(r.page, 1);
        QCOMPARE(r.totalPages, 42);
        QCOMPARE(r.totalResults, 831);
    }

    void parsePagedList_throws_on_non_object()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("[]"));
        QVERIFY_EXCEPTION_THROWN(
            tmdb::parsePagedList(doc, MediaKind::Movie),
            kinema::core::HttpError);
    }

    void parseGenreList_skips_rows_missing_id_or_name()
    {
        const auto doc = loadFixture("tmdb_genre_movie_list.json");
        const auto genres = tmdb::parseGenreList(doc);

        // Fixture has 8 entries; 2 are invalid (id=0, empty name).
        QCOMPARE(genres.size(), 6);
        QCOMPARE(genres.at(0).id, 28);
        QCOMPARE(genres.at(0).name, QStringLiteral("Action"));
        QCOMPARE(genres.at(5).id, 18);
        QCOMPARE(genres.at(5).name, QStringLiteral("Drama"));
    }

    void parseGenreList_throws_on_non_object()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("[]"));
        QVERIFY_EXCEPTION_THROWN(
            tmdb::parseGenreList(doc),
            kinema::core::HttpError);
    }

    void parseGenreList_missing_array_returns_empty()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        QVERIFY(tmdb::parseGenreList(doc).isEmpty());
    }
};

QTEST_APPLESS_MAIN(TstTmdbParse)
#include "tst_tmdb_parse.moc"
