// SPDX-License-Identifier: Apache-2.0

#include "torrent/PiecePlanner.h"

#include <QTest>

using namespace kinema;

class TstPiecePlanner : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void mapsFileBytesToTorrentPieces()
    {
        torrent::FilePieceLayout layout;
        layout.fileOffset = 1024;
        layout.fileSize = 4096;
        layout.pieceSize = 1024;
        layout.pieceCount = 10;

        const auto r = torrent::pieceRangeForBytes(layout, {0, 2047});
        QVERIFY(r.isValid());
        QCOMPARE(r.first, 1);
        QCOMPARE(r.last, 2);
    }

    void startupIncludesHeadAndTail()
    {
        torrent::FilePieceLayout layout;
        layout.fileOffset = 0;
        layout.fileSize = 10 * 1024;
        layout.pieceSize = 1024;
        layout.pieceCount = 10;

        const auto ranges = torrent::startupPieceWindows(layout, 2048, 1024);
        QCOMPARE(ranges.size(), 2);
        QCOMPARE(ranges.first().first, 0);
        QCOMPARE(ranges.first().last, 1);
        QCOMPARE(ranges.last().first, 9);
        QCOMPARE(ranges.last().last, 9);
    }
};

QTEST_MAIN(TstPiecePlanner)
#include "tst_piece_planner.moc"
