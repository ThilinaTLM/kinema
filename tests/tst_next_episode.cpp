// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "core/NextEpisode.h"

#include <QtTest>

using kinema::api::Episode;
using kinema::api::MediaKind;
using kinema::api::PlaybackKey;
using kinema::core::series::nextEpisodeKey;

namespace {

Episode makeEpisode(int season, int number, QString title = {})
{
    Episode ep;
    ep.season = season;
    ep.number = number;
    ep.title = std::move(title);
    return ep;
}

PlaybackKey makeEpisodeKey(const QString& imdbId, int season, int episode)
{
    PlaybackKey key;
    key.kind = MediaKind::Series;
    key.imdbId = imdbId;
    key.season = season;
    key.episode = episode;
    return key;
}

} // namespace

class TstNextEpisode : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testReturnsNullForInvalidOrNonSeries()
    {
        QList<Episode> episodes {
            makeEpisode(1, 1),
            makeEpisode(1, 2),
        };

        PlaybackKey movie;
        movie.kind = MediaKind::Movie;
        movie.imdbId = QStringLiteral("tt0133093");
        QVERIFY(!nextEpisodeKey(movie, episodes).has_value());

        PlaybackKey invalid;
        invalid.kind = MediaKind::Series;
        invalid.imdbId = QStringLiteral("tt0944947");
        invalid.season = 1;
        QVERIFY(!nextEpisodeKey(invalid, episodes).has_value());
    }

    void testReturnsNextEpisodeInSameSeason()
    {
        QList<Episode> episodes {
            makeEpisode(1, 1),
            makeEpisode(1, 2),
            makeEpisode(1, 3),
        };

        const auto next = nextEpisodeKey(
            makeEpisodeKey(QStringLiteral("tt0903747"), 1, 2), episodes);

        QVERIFY(next.has_value());
        QCOMPARE(next->kind, MediaKind::Series);
        QCOMPARE(next->imdbId, QStringLiteral("tt0903747"));
        QCOMPARE(next->season, std::optional<int>(1));
        QCOMPARE(next->episode, std::optional<int>(3));
    }

    void testWrapsAcrossSeasons()
    {
        QList<Episode> episodes {
            makeEpisode(1, 9),
            makeEpisode(1, 10),
            makeEpisode(2, 1),
            makeEpisode(2, 2),
        };

        const auto next = nextEpisodeKey(
            makeEpisodeKey(QStringLiteral("tt2861424"), 1, 10), episodes);

        QVERIFY(next.has_value());
        QCOMPARE(next->season, std::optional<int>(2));
        QCOMPARE(next->episode, std::optional<int>(1));
    }

    void testReturnsNulloptAtSeriesFinale()
    {
        QList<Episode> episodes {
            makeEpisode(1, 1),
            makeEpisode(1, 2),
        };

        QVERIFY(!nextEpisodeKey(
            makeEpisodeKey(QStringLiteral("tt0412142"), 1, 2), episodes)
                     .has_value());
    }

    void testSkipsSpecialsAndEpisodeGaps()
    {
        QList<Episode> episodes {
            makeEpisode(0, 1),
            makeEpisode(1, 1),
            makeEpisode(1, 3),
            makeEpisode(2, 1),
        };

        const auto next = nextEpisodeKey(
            makeEpisodeKey(QStringLiteral("tt1520211"), 1, 1), episodes);
        QVERIFY(next.has_value());
        QCOMPARE(next->season, std::optional<int>(1));
        QCOMPARE(next->episode, std::optional<int>(3));

        const auto afterGap = nextEpisodeKey(
            makeEpisodeKey(QStringLiteral("tt1520211"), 1, 3), episodes);
        QVERIFY(afterGap.has_value());
        QCOMPARE(afterGap->season, std::optional<int>(2));
        QCOMPARE(afterGap->episode, std::optional<int>(1));
    }
};

QTEST_MAIN(TstNextEpisode)
#include "tst_next_episode.moc"
