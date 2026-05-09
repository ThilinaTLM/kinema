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
        api::PlaybackContext ctx;
        ctx.key.kind = api::MediaKind::Movie;

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
        api::PlaybackContext ctx;
        ctx.key.kind = api::MediaKind::Series;
        ctx.key.season = 1;
        ctx.key.episode = 2;

        const auto picked = torrent::selectMediaFile(files, ctx);
        QVERIFY(picked.ok());
        QCOMPARE(picked.file->index, 1);
    }
};

QTEST_MAIN(TstMediaFileSelector)
#include "tst_media_file_selector.moc"
