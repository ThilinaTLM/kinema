// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/TmdbDiscoverUrl.h"
#include "api/Types.h"
#include "core/DateWindow.h"

#include <QDate>
#include <QTest>
#include <QUrlQuery>

using namespace kinema::api;
using kinema::core::DateRange;
using kinema::core::DateWindow;
using kinema::core::dateRangeFor;
using kinema::core::dateWindowFromString;
using kinema::core::dateWindowToString;

class TstDiscoverQuery : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    // ---- dateRangeFor -----------------------------------------------------

    void dateWindow_pastMonth_clamps_lte_to_today()
    {
        const QDate today(2026, 4, 21);
        const auto r = dateRangeFor(DateWindow::PastMonth, today);
        QVERIFY(r.gte);
        QVERIFY(r.lte);
        QCOMPARE(*r.gte, QDate(2026, 3, 21));
        QCOMPARE(*r.lte, today);
    }

    void dateWindow_past3Months_produces_quarter_range()
    {
        const QDate today(2026, 4, 21);
        const auto r = dateRangeFor(DateWindow::Past3Months, today);
        QCOMPARE(*r.gte, QDate(2026, 1, 21));
        QCOMPARE(*r.lte, today);
    }

    void dateWindow_thisYear_anchors_to_jan_1()
    {
        const QDate today(2026, 4, 21);
        const auto r = dateRangeFor(DateWindow::ThisYear, today);
        QCOMPARE(*r.gte, QDate(2026, 1, 1));
        QCOMPARE(*r.lte, today);
    }

    void dateWindow_past3Years_shifts_year_back()
    {
        const QDate today(2026, 4, 21);
        const auto r = dateRangeFor(DateWindow::Past3Years, today);
        QCOMPARE(*r.gte, QDate(2023, 4, 21));
        QCOMPARE(*r.lte, today);
    }

    void dateWindow_any_has_no_bounds()
    {
        const auto r = dateRangeFor(DateWindow::Any, QDate(2026, 4, 21));
        QVERIFY(!r.gte.has_value());
        QVERIFY(!r.lte.has_value());
    }

    void dateWindow_roundtrip_through_config_tokens()
    {
        for (auto w : { DateWindow::PastMonth, DateWindow::Past3Months,
                        DateWindow::ThisYear, DateWindow::Past3Years,
                        DateWindow::Any }) {
            QCOMPARE(dateWindowFromString(dateWindowToString(w)), w);
        }
    }

    void dateWindow_unknown_token_falls_back()
    {
        QCOMPARE(dateWindowFromString(QStringLiteral("garbage")),
            DateWindow::ThisYear);
        QCOMPARE(dateWindowFromString(QStringLiteral("garbage"),
                     DateWindow::Any),
            DateWindow::Any);
    }

    // ---- discoverPath / discoverSortValue --------------------------------

    void discoverPath_branches_on_kind()
    {
        QCOMPARE(tmdb::discoverPath(MediaKind::Movie),
            QStringLiteral("/discover/movie"));
        QCOMPARE(tmdb::discoverPath(MediaKind::Series),
            QStringLiteral("/discover/tv"));
    }

    void discoverSortValue_popularity_is_kind_independent()
    {
        QCOMPARE(tmdb::discoverSortValue(MediaKind::Movie,
                     DiscoverSort::Popularity),
            QStringLiteral("popularity.desc"));
        QCOMPARE(tmdb::discoverSortValue(MediaKind::Series,
                     DiscoverSort::Popularity),
            QStringLiteral("popularity.desc"));
    }

    void discoverSortValue_releaseDate_switches_field_per_kind()
    {
        QCOMPARE(tmdb::discoverSortValue(MediaKind::Movie,
                     DiscoverSort::ReleaseDate),
            QStringLiteral("primary_release_date.desc"));
        QCOMPARE(tmdb::discoverSortValue(MediaKind::Series,
                     DiscoverSort::ReleaseDate),
            QStringLiteral("first_air_date.desc"));
    }

    void discoverSortValue_title_switches_field_per_kind()
    {
        QCOMPARE(tmdb::discoverSortValue(MediaKind::Movie,
                     DiscoverSort::TitleAsc),
            QStringLiteral("original_title.asc"));
        QCOMPARE(tmdb::discoverSortValue(MediaKind::Series,
                     DiscoverSort::TitleAsc),
            QStringLiteral("name.asc"));
    }

    // ---- discoverQueryToQuery --------------------------------------------

    void query_minimal_movie_has_adult_video_sort()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("include_adult")),
            QStringLiteral("false"));
        QCOMPARE(u.queryItemValue(QStringLiteral("include_video")),
            QStringLiteral("false"));
        QCOMPARE(u.queryItemValue(QStringLiteral("sort_by")),
            QStringLiteral("popularity.desc"));
        QVERIFY(!u.hasQueryItem(QStringLiteral("page")));
        QVERIFY(!u.hasQueryItem(QStringLiteral("with_genres")));
        QVERIFY(!u.hasQueryItem(QStringLiteral("vote_count.gte")));
    }

    void query_minimal_tv_omits_include_video()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Series;
        const auto u = tmdb::discoverQueryToQuery(q);
        QVERIFY(!u.hasQueryItem(QStringLiteral("include_video")));
        QCOMPARE(u.queryItemValue(QStringLiteral("include_adult")),
            QStringLiteral("false"));
    }

    void query_genres_joined_with_comma()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.withGenreIds = { 28, 12, 35 };
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("with_genres")),
            QStringLiteral("28,12,35"));
    }

    void query_date_range_uses_movie_fields()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.releasedGte = QDate(2026, 1, 1);
        q.releasedLte = QDate(2026, 4, 21);
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(
                     QStringLiteral("primary_release_date.gte")),
            QStringLiteral("2026-01-01"));
        QCOMPARE(u.queryItemValue(
                     QStringLiteral("primary_release_date.lte")),
            QStringLiteral("2026-04-21"));
        QVERIFY(!u.hasQueryItem(QStringLiteral("first_air_date.gte")));
    }

    void query_date_range_uses_tv_fields()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Series;
        q.releasedGte = QDate(2025, 1, 1);
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("first_air_date.gte")),
            QStringLiteral("2025-01-01"));
        QVERIFY(!u.hasQueryItem(
            QStringLiteral("primary_release_date.gte")));
    }

    void query_rating_sort_auto_adds_vote_count_floor()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.sort = DiscoverSort::Rating;
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("sort_by")),
            QStringLiteral("vote_average.desc"));
        QCOMPARE(u.queryItemValue(QStringLiteral("vote_count.gte")),
            QStringLiteral("200"));
    }

    void query_rating_sort_keeps_higher_user_floor()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.sort = DiscoverSort::Rating;
        q.voteCountGte = 500;
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("vote_count.gte")),
            QStringLiteral("500"));
    }

    void query_non_rating_sort_respects_user_floor_only()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.sort = DiscoverSort::Popularity;
        q.voteCountGte = 200;
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("vote_count.gte")),
            QStringLiteral("200"));
    }

    void query_non_rating_sort_without_floor_omits_it()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.sort = DiscoverSort::Popularity;
        const auto u = tmdb::discoverQueryToQuery(q);
        QVERIFY(!u.hasQueryItem(QStringLiteral("vote_count.gte")));
    }

    void query_vote_average_formatted_to_one_decimal()
    {
        DiscoverQuery q;
        q.kind = MediaKind::Movie;
        q.voteAverageGte = 7.5;
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("vote_average.gte")),
            QStringLiteral("7.5"));
    }

    void query_page_omitted_when_one()
    {
        DiscoverQuery q;
        q.page = 1;
        const auto u = tmdb::discoverQueryToQuery(q);
        QVERIFY(!u.hasQueryItem(QStringLiteral("page")));
    }

    void query_page_included_when_greater_than_one()
    {
        DiscoverQuery q;
        q.page = 3;
        const auto u = tmdb::discoverQueryToQuery(q);
        QCOMPARE(u.queryItemValue(QStringLiteral("page")),
            QStringLiteral("3"));
    }
};

QTEST_APPLESS_MAIN(TstDiscoverQuery)
#include "tst_discover_query.moc"
