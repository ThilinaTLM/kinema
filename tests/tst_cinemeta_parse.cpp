// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/CinemetaParse.h"
#include "core/HttpError.h"

#include <QFile>
#include <QJsonDocument>
#include <QTest>

using namespace kinema::api;

class TstCinemetaParse : public QObject
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
    void search_filtersToRequestedKindAndValidRows()
    {
        const auto doc = loadFixture("cinemeta_search_matrix.json");
        const auto results = cinemeta::parseSearch(doc, MediaKind::Movie);

        // Two valid movies (the series row and the empty-id row are filtered).
        QCOMPARE(results.size(), 2);
        QCOMPARE(results.at(0).imdbId, QStringLiteral("tt0133093"));
        QCOMPARE(results.at(0).title, QStringLiteral("The Matrix"));
        QCOMPARE(results.at(0).kind, MediaKind::Movie);
        QVERIFY(results.at(0).year.has_value());
        QCOMPARE(*results.at(0).year, 1999);
        QVERIFY(results.at(0).imdbRating.has_value());
        QCOMPARE(*results.at(0).imdbRating, 8.7);
        QCOMPARE(results.at(0).poster, QUrl(QStringLiteral("https://example.com/matrix.jpg")));
    }

    void search_throwsOnNonObjectDocument()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("[]"));
        QVERIFY_EXCEPTION_THROWN(
            cinemeta::parseSearch(doc, MediaKind::Movie),
            kinema::core::HttpError);
    }

    void search_emptyMetasReturnsEmpty_notError()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        const auto results = cinemeta::parseSearch(doc, MediaKind::Movie);
        QVERIFY(results.isEmpty());
    }

    void meta_parsesRuntimeFromString()
    {
        const auto doc = loadFixture("cinemeta_meta_tt0133093.json");
        const auto d = cinemeta::parseMeta(doc, MediaKind::Movie);
        QCOMPARE(d.summary.imdbId, QStringLiteral("tt0133093"));
        QVERIFY(d.runtimeMinutes.has_value());
        QCOMPARE(*d.runtimeMinutes, 136);
        QCOMPARE(d.genres.size(), 2);
        QCOMPARE(d.genres.first(), QStringLiteral("Action"));
        QCOMPARE(d.cast.size(), 3);
        QCOMPARE(d.background,
            QUrl(QStringLiteral("https://example.com/matrix_bg.jpg")));
    }

    void meta_throwsOnMissingMetaObject()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{\"meta\": null}"));
        QVERIFY_EXCEPTION_THROWN(
            cinemeta::parseMeta(doc, MediaKind::Movie),
            kinema::core::HttpError);
    }

    void series_parsesMetaAndEpisodes()
    {
        const auto doc = loadFixture("cinemeta_meta_tt0903747_breaking_bad.json");
        const auto sd = cinemeta::parseSeriesMeta(doc);

        QCOMPARE(sd.meta.summary.imdbId, QStringLiteral("tt0903747"));
        QCOMPARE(sd.meta.summary.kind, MediaKind::Series);
        QCOMPARE(sd.meta.summary.title, QStringLiteral("Breaking Bad"));
        QCOMPARE(sd.meta.genres.size(), 3);

        // Top-level `released` is parsed into MetaSummary.released.
        QVERIFY(sd.meta.summary.released.has_value());
        QCOMPARE(*sd.meta.summary.released, QDate(2008, 1, 20));

        // Fixture has 1 special + 3 S1 + 4 S2 = 8 valid episodes.
        // The 9th "bad" row is dropped because it lacks season/number.
        QCOMPARE(sd.episodes.size(), 8);

        // Sorted by (season, number): specials first, then S1, then S2.
        QCOMPARE(sd.episodes.at(0).season, 0);
        QCOMPARE(sd.episodes.at(1).season, 1);
        QCOMPARE(sd.episodes.at(1).number, 1);
        QCOMPARE(sd.episodes.at(1).title, QStringLiteral("Pilot"));
        QVERIFY(sd.episodes.at(1).released.has_value());
        QCOMPARE(*sd.episodes.at(1).released, QDate(2008, 1, 20));
    }

    void series_streamIdMatchesTorrentioShape()
    {
        const auto doc = loadFixture("cinemeta_meta_tt0903747_breaking_bad.json");
        const auto sd = cinemeta::parseSeriesMeta(doc);

        // Find S1E1.
        const auto pilot = std::find_if(sd.episodes.begin(), sd.episodes.end(),
            [](const Episode& e) { return e.season == 1 && e.number == 1; });
        QVERIFY(pilot != sd.episodes.end());
        QCOMPARE(pilot->streamId(QStringLiteral("tt0903747")),
            QStringLiteral("tt0903747:1:1"));
    }

    void series_seasonNumbersExcludesSpecials()
    {
        const auto doc = loadFixture("cinemeta_meta_tt0903747_breaking_bad.json");
        const auto sd = cinemeta::parseSeriesMeta(doc);

        const auto seasons = cinemeta::seasonNumbers(sd.episodes);
        QCOMPARE(seasons, (QList<int> { 1, 2 }));
    }

    void series_toleratesDateOnlyAndMissingThumbnail()
    {
        const auto doc = loadFixture("cinemeta_meta_tt0903747_breaking_bad.json");
        const auto sd = cinemeta::parseSeriesMeta(doc);

        // S1E2 has "released": "2008-01-27" (date-only).
        const auto s1e2 = std::find_if(sd.episodes.begin(), sd.episodes.end(),
            [](const Episode& e) { return e.season == 1 && e.number == 2; });
        QVERIFY(s1e2 != sd.episodes.end());
        QVERIFY(s1e2->released.has_value());
        QCOMPARE(*s1e2->released, QDate(2008, 1, 27));

        // S1E3 has an empty thumbnail field.
        const auto s1e3 = std::find_if(sd.episodes.begin(), sd.episodes.end(),
            [](const Episode& e) { return e.season == 1 && e.number == 3; });
        QVERIFY(s1e3 != sd.episodes.end());
        QVERIFY(s1e3->thumbnail.isEmpty());
    }

    void series_acceptsLiveNameAndFirstAiredKeys()
    {
        // Live Cinemeta uses `name` and `firstAired`; the legacy shape
        // our other fixture rows use is `title` and `released`. S2E4
        // in the fixture exercises the live shape; the parser must
        // accept both.
        const auto doc = loadFixture("cinemeta_meta_tt0903747_breaking_bad.json");
        const auto sd = cinemeta::parseSeriesMeta(doc);

        const auto down = std::find_if(sd.episodes.begin(), sd.episodes.end(),
            [](const Episode& e) { return e.season == 2 && e.number == 4; });
        QVERIFY(down != sd.episodes.end());
        QCOMPARE(down->title, QStringLiteral("Down"));
        QVERIFY(down->released.has_value());
        QCOMPARE(*down->released, QDate(2009, 3, 29));
    }
};

QTEST_APPLESS_MAIN(TstCinemetaParse)
#include "tst_cinemeta_parse.moc"
