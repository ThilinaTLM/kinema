// SPDX-License-Identifier: Apache-2.0

#include "torrent/MediaFileSelector.h"

#include <QTest>

using namespace kinema;

class TstMediaFileSelector : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void moviePicksLargestVideoAndSkipsSamples()
    {
        QVector<torrent::TorrentFileEntry> files {
            {0, QStringLiteral("Movie/sample.mkv"), 10LL * 1024 * 1024},
            {1, QStringLiteral("Movie/Movie.1080p.mkv"), 2LL * 1024 * 1024 * 1024},
            {2, QStringLiteral("Movie/Movie.720p.mp4"), 1LL * 1024 * 1024 * 1024},
        };
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;

        const auto picked = torrent::selectMediaFile(files, ctx);
        QVERIFY(picked.ok());
        QCOMPARE(picked.file->index, 1);
    }

    void seriesMatchesEpisodeToken()
    {
        QVector<torrent::TorrentFileEntry> files {
            {0, QStringLiteral("Show/Show.S01E01.mkv"), 800LL * 1024 * 1024},
            {1, QStringLiteral("Show/Show.S01E02.mkv"), 700LL * 1024 * 1024},
        };
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Series;
        ctx.key.season = 1;
        ctx.key.episode = 2;

        const auto picked = torrent::selectMediaFile(files, ctx);
        QVERIFY(picked.ok());
        QCOMPARE(picked.file->index, 1);
    }

    void adjacentEpisodesFoundForSeasonPack()
    {
        QVector<torrent::TorrentFileEntry> files {
            {0, QStringLiteral("Show/Show.S01E01.mkv"), 800LL * 1024 * 1024},
            {1, QStringLiteral("Show/Show.S01E02.mkv"), 810LL * 1024 * 1024},
            {2, QStringLiteral("Show/Show.S01E03.mkv"), 820LL * 1024 * 1024},
        };

        const auto nav = torrent::adjacentEpisodeFiles(files, 1, 2);
        QVERIFY(nav.has_value());
        QVERIFY(nav->current.has_value());
        QVERIFY(nav->previous.has_value());
        QVERIFY(nav->next.has_value());
        QCOMPARE(nav->previous->file.index, 0);
        QCOMPARE(nav->current->file.index, 1);
        QCOMPARE(nav->next->file.index, 2);
    }

    void ambiguousCurrentEpisodeDisablesNavigation()
    {
        QVector<torrent::TorrentFileEntry> files {
            {0, QStringLiteral("Show/Show.S01E02.1080p.mkv"), 800LL * 1024 * 1024},
            {1, QStringLiteral("Show/Show.S01E02.720p.mkv"), 600LL * 1024 * 1024},
            {2, QStringLiteral("Show/Show.S01E03.mkv"), 820LL * 1024 * 1024},
        };

        const auto nav = torrent::adjacentEpisodeFiles(files, 1, 2);
        QVERIFY(!nav.has_value());
    }

    void edgeEpisodesHaveAsymmetricNavigation()
    {
        QVector<torrent::TorrentFileEntry> files {
            {0, QStringLiteral("Show/Show.S02E01.mkv"), 800LL * 1024 * 1024},
            {1, QStringLiteral("Show/Show.S02E02.mkv"), 810LL * 1024 * 1024},
            {2, QStringLiteral("Show/Show.S02E03.mkv"), 820LL * 1024 * 1024},
            {3, QStringLiteral("Show/Show.S02E04.mkv"), 830LL * 1024 * 1024},
            {4, QStringLiteral("Show/Show.S02E05.mkv"), 840LL * 1024 * 1024},
        };

        // First episode: no `previous`, `next` set.
        auto nav = torrent::adjacentEpisodeFiles(files, 2, 1);
        QVERIFY(nav.has_value());
        QVERIFY(nav->current.has_value());
        QVERIFY(!nav->previous.has_value());
        QVERIFY(nav->next.has_value());
        QCOMPARE(nav->next->episode, 2);

        // Middle episode: both `previous` and `next` set.
        nav = torrent::adjacentEpisodeFiles(files, 2, 3);
        QVERIFY(nav.has_value());
        QVERIFY(nav->previous.has_value());
        QCOMPARE(nav->previous->episode, 2);
        QVERIFY(nav->next.has_value());
        QCOMPARE(nav->next->episode, 4);

        // Last episode: `previous` set, `next` absent.
        nav = torrent::adjacentEpisodeFiles(files, 2, 5);
        QVERIFY(nav.has_value());
        QVERIFY(nav->previous.has_value());
        QCOMPARE(nav->previous->episode, 4);
        QVERIFY(!nav->next.has_value());
    }

    void playableCandidateCountSkipsNonVideoAndSamples()
    {
        QVector<torrent::TorrentFileEntry> files {
            {0, QStringLiteral("Show/Show.S01E01.mkv"), 800LL * 1024 * 1024},
            {1, QStringLiteral("Show/Show.S01E02.mkv"), 800LL * 1024 * 1024},
            {2, QStringLiteral("Show/Sample/sample.mkv"), 30LL * 1024 * 1024},
            {3, QStringLiteral("Show/Show.S01E03.mkv"), 800LL * 1024 * 1024},
            {4, QStringLiteral("Show/Show.nfo"), 4096},
            {5, QStringLiteral("Show/Show.S01E04.trailer.mkv"), 80LL * 1024 * 1024},
        };
        QCOMPARE(torrent::playableCandidateCount(files), 3);
    }

    void playableCandidateCountIsZeroForEmptyAndMovieOnly()
    {
        QCOMPARE(torrent::playableCandidateCount({}), 0);

        QVector<torrent::TorrentFileEntry> files {
            {0, QStringLiteral("Movie/Movie.1080p.mkv"),
                2LL * 1024 * 1024 * 1024},
        };
        QCOMPARE(torrent::playableCandidateCount(files), 1);
    }
};

QTEST_MAIN(TstMediaFileSelector)
#include "tst_media_file_selector.moc"
