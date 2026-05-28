// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/StreamingPriorityPolicy.h"

#include <QTest>

using namespace kinema::playback::policy;

namespace {

kinema::torrent::FilePieceLayout layout(qint64 size, int pieceSize,
    qint64 offset = 0)
{
    kinema::torrent::FilePieceLayout l;
    l.fileOffset = offset;
    l.fileSize = size;
    l.pieceSize = pieceSize;
    l.pieceCount = static_cast<int>((size + pieceSize - 1) / pieceSize);
    return l;
}

} // namespace

class TstStreamingPriorityPolicy : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void startupHeadAndTail()
    {
        StreamingPriorityInputs in;
        in.layout = layout(64 * 1024 * 1024, 1024 * 1024);
        in.startupBufferBytes = 4 * 1024 * 1024;
        in.tailBufferBytes = 2 * 1024 * 1024;
        const auto plans = planStartupDeadlines(in);
        QVERIFY(plans.size() == 2);
        QCOMPARE(plans.first().first, 0);
        // 4 MiB / 1 MiB pieces = 4 pieces (0..3)
        QCOMPARE(plans.first().last, 3);
        // step 10 ms
        QCOMPARE(plans.first().stepMs, 10);
        // Tail block
        QVERIFY(plans.last().first > plans.first().last);
    }

    void readaheadCoversAtLeastRange()
    {
        StreamingPriorityInputs in;
        in.layout = layout(64 * 1024 * 1024, 1024 * 1024);
        in.readaheadBytes = 4 * 1024 * 1024;
        const auto d = planReadaheadDeadlines(in,
            10 * 1024 * 1024, 12 * 1024 * 1024);
        QVERIFY(d.isValid());
        // Request piece range starts at 10
        QCOMPARE(d.first, 10);
        // readahead pushes the end out by ~4 MiB beyond requestEnd
        QVERIFY(d.last >= 12);
        QCOMPARE(d.stepMs, 5);
    }

    void requiredPiecesMatchRange()
    {
        const auto l = layout(64 * 1024 * 1024, 1024 * 1024);
        const auto pr = requiredPiecesForRange(l, 2 * 1024 * 1024,
            5 * 1024 * 1024);
        QCOMPARE(pr.first, 2);
        QCOMPARE(pr.last, 5);
    }
};

QTEST_MAIN(TstStreamingPriorityPolicy)
#include "tst_streaming_priority_policy.moc"
