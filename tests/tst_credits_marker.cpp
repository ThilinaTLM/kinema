// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/mpv/MpvChapterList.h"

#include <QTest>

using namespace kinema::core::chapters;

namespace {

Chapter mk(double time, const QString& title = {})
{
    Chapter c;
    c.time = time;
    c.title = title;
    return c;
}

} // namespace

class TstCreditsMarker : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    // ---- Titled match (rule 1) ------------------------------------------

    void testTitledEndCreditsMatches()
    {
        const ChapterList chapters = {
            mk(0.0, QStringLiteral("Cold Open")),
            mk(600.0, QStringLiteral("Act II")),
            mk(5400.0, QStringLiteral("End Credits")),
        };
        const auto result = findCreditsStart(chapters, 6000.0);
        QVERIFY(result.has_value());
        QCOMPARE(*result, 5400.0);
    }

    void testTitledCreditsCaseInsensitive()
    {
        const ChapterList lower = {
            mk(0.0), mk(5400.0, QStringLiteral("credits")),
        };
        QVERIFY(findCreditsStart(lower, 6000.0).has_value());

        const ChapterList upper = {
            mk(0.0), mk(5400.0, QStringLiteral("ENDING")),
        };
        QVERIFY(findCreditsStart(upper, 6000.0).has_value());

        const ChapterList mixed = {
            mk(0.0), mk(5400.0, QStringLiteral("End Titles")),
        };
        QVERIFY(findCreditsStart(mixed, 6000.0).has_value());

        const ChapterList card = {
            mk(0.0), mk(5400.0, QStringLiteral("End Card")),
        };
        QVERIFY(findCreditsStart(card, 6000.0).has_value());
    }

    void testBareEndDoesNotMatch()
    {
        // "End" alone is intentionally not a match — too many false
        // positives on act/finale chapter names.
        const ChapterList chapters = {
            mk(0.0, QStringLiteral("Intro")),
            mk(2400.0, QStringLiteral("The End")),
        };
        // Duration 3000 -> chapter at 0.80, within untitled-tail
        // window BUT it's not the *last* chapter rule case because
        // the tail-window match itself succeeds; what we're checking
        // is that the title regex does not fire on "The End". Use
        // a single-chapter list to disable the heuristic.
        const ChapterList singleton = {
            mk(2400.0, QStringLiteral("The End")),
        };
        QVERIFY(!findCreditsStart(singleton, 3000.0).has_value());
        Q_UNUSED(chapters);
    }

    void testEndingWithSurroundingTextStillMatches()
    {
        const ChapterList chapters = {
            mk(0.0), mk(5400.0, QStringLiteral("Show Ending Scroll")),
        };
        QVERIFY(findCreditsStart(chapters, 6000.0).has_value());
    }

    // ---- Untitled last-chapter heuristic (rule 2) -----------------------

    void testUntitledLastChapterInTailMatches()
    {
        // Last chapter at 88% of duration — within [80%, 97%].
        const ChapterList chapters = {
            mk(0.0), mk(2000.0), mk(5280.0),
        };
        const auto result = findCreditsStart(chapters, 6000.0);
        QVERIFY(result.has_value());
        QCOMPARE(*result, 5280.0);
    }

    void testUntitledChapterTooEarlyDoesNotMatch()
    {
        // Last chapter at 70% — outside window.
        const ChapterList chapters = {
            mk(0.0), mk(2000.0), mk(4200.0),
        };
        QVERIFY(!findCreditsStart(chapters, 6000.0).has_value());
    }

    void testUntitledChapterTooLateDoesNotMatch()
    {
        // Last chapter at 99% — likely a post-credits stinger, not
        // the credits boundary itself.
        const ChapterList chapters = {
            mk(0.0), mk(2000.0), mk(5940.0),
        };
        QVERIFY(!findCreditsStart(chapters, 6000.0).has_value());
    }

    void testUntitledHeuristicRequiresAtLeastTwoChapters()
    {
        // A single chapter at 88% is meaningless on its own — most
        // files have an implicit "chapter 1 at 0" so a sole entry
        // probably isn't credits.
        const ChapterList chapters = {
            mk(5280.0),
        };
        QVERIFY(!findCreditsStart(chapters, 6000.0).has_value());
    }

    // ---- Combined / fallbacks -------------------------------------------

    void testTitledMatchBeatsUntitledHeuristic()
    {
        // Last chapter (without a credits title) sits inside the
        // tail window, but an earlier chapter is titled "Credits" —
        // the titled match wins.
        const ChapterList chapters = {
            mk(0.0, QStringLiteral("Opening")),
            mk(4800.0, QStringLiteral("Credits")),
            mk(5400.0, QStringLiteral("Stinger")),
        };
        const auto result = findCreditsStart(chapters, 6000.0);
        QVERIFY(result.has_value());
        QCOMPARE(*result, 4800.0);
    }

    void testEmptyChapterListReturnsNullopt()
    {
        QVERIFY(!findCreditsStart({}, 6000.0).has_value());
    }

    void testZeroDurationDisablesUntitledHeuristicOnly()
    {
        // No duration -> only rule (1) fires.
        const ChapterList untitled = {
            mk(0.0), mk(5280.0),
        };
        QVERIFY(!findCreditsStart(untitled, 0.0).has_value());

        const ChapterList titled = {
            mk(0.0), mk(5400.0, QStringLiteral("End Credits")),
        };
        const auto result = findCreditsStart(titled, 0.0);
        QVERIFY(result.has_value());
        QCOMPARE(*result, 5400.0);
    }
};

QTEST_GUILESS_MAIN(TstCreditsMarker)
#include "tst_credits_marker.moc"
