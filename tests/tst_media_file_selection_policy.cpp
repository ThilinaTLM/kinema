// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/MediaFileSelectionPolicy.h"

#include <QTest>

using namespace kinema;
using namespace kinema::playback::policy;

namespace {

domain::MediaFileEntry mk(int idx, const QString& path, qint64 size)
{
    domain::MediaFileEntry e;
    e.index = idx;
    e.path = path;
    e.size = size;
    return e;
}

} // namespace

class TstMediaFileSelectionPolicy : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void moviePicksLargestVideoAndSkipsSamples()
    {
        QVector<domain::MediaFileEntry> files {
            mk(0, QStringLiteral("Movie/sample.mkv"), 10LL * 1024 * 1024),
            mk(1, QStringLiteral("Movie/Movie.1080p.mkv"), 2LL * 1024 * 1024 * 1024),
            mk(2, QStringLiteral("Movie/Movie.720p.mp4"), 1LL * 1024 * 1024 * 1024),
        };
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;

        const auto picked = selectMediaFile(files, ctx);
        QVERIFY(picked.ok());
        QCOMPARE(picked.file->index, 1);
    }

    void seriesMatchesEpisodeToken()
    {
        QVector<domain::MediaFileEntry> files {
            mk(0, QStringLiteral("Show/Show.S01E01.mkv"),
                500LL * 1024 * 1024),
            mk(1, QStringLiteral("Show/Show.S01E02.mkv"),
                500LL * 1024 * 1024),
        };
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Series;
        ctx.key.imdbId = QStringLiteral("tt123");
        ctx.key.season = 1;
        ctx.key.episode = 2;

        const auto picked = selectMediaFile(files, ctx);
        QVERIFY(picked.ok());
        QCOMPARE(picked.file->index, 1);
    }

    void adjacentEpisodeFilesBuildsPrevNext()
    {
        QVector<domain::MediaFileEntry> files {
            mk(0, QStringLiteral("Show.S01E01.mkv"), 500LL * 1024 * 1024),
            mk(1, QStringLiteral("Show.S01E02.mkv"), 500LL * 1024 * 1024),
            mk(2, QStringLiteral("Show.S01E03.mkv"), 500LL * 1024 * 1024),
        };
        const auto nav = adjacentEpisodeFiles(files, 1, 2);
        QVERIFY(nav.has_value());
        QVERIFY(nav->current.has_value());
        QVERIFY(nav->previous.has_value());
        QVERIFY(nav->next.has_value());
        QCOMPARE(nav->previous->episode, 1);
        QCOMPARE(nav->next->episode, 3);
    }

    void ambiguousCurrentClearsNavigation()
    {
        QVector<domain::MediaFileEntry> files {
            mk(0, QStringLiteral("Show.S01E02.1080p.mkv"),
                500LL * 1024 * 1024),
            mk(1, QStringLiteral("Show.S01E02.720p.mkv"),
                500LL * 1024 * 1024),
            mk(2, QStringLiteral("Show.S01E03.mkv"), 500LL * 1024 * 1024),
        };
        QVERIFY(!adjacentEpisodeFiles(files, 1, 2).has_value());
    }

    void emptyOrTinySkipped()
    {
        QVector<domain::MediaFileEntry> files {
            mk(0, QStringLiteral("Movie/sample.mkv"), 10LL * 1024 * 1024),
            mk(1, QStringLiteral("Movie/trailer.mp4"), 200LL * 1024 * 1024),
        };
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;
        const auto picked = selectMediaFile(files, ctx);
        QVERIFY(!picked.ok());
    }
};

QTEST_MAIN(TstMediaFileSelectionPolicy)
#include "tst_media_file_selection_policy.moc"
