// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/ResumePolicy.h"

#include <QTest>

using namespace kinema;
using namespace kinema::playback::policy;

namespace {

domain::PlaybackKey makeKey()
{
    domain::PlaybackKey k;
    k.kind = domain::MediaKind::Movie;
    k.imdbId = QStringLiteral("tt001");
    return k;
}

domain::HistoryEntry makeStored(double pos, double dur, bool finished = false)
{
    domain::HistoryEntry e;
    e.key = makeKey();
    e.positionSec = pos;
    e.durationSec = dur;
    e.finished = finished;
    return e;
}

} // namespace

class TstResumePolicy : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void invalidKeyReturnsNullopt()
    {
        ResumeInputs in;
        QVERIFY(!resumeSecondsFor(in).has_value());
    }

    void activeSessionWins()
    {
        ResumeInputs in;
        in.key = makeKey();
        in.activeSessionMatches = true;
        in.activeSessionPositionSec = 42.0;
        in.storedEntry = makeStored(10.0, 3600.0);
        const auto r = resumeSecondsFor(in);
        QVERIFY(r.has_value());
        QCOMPARE(*r, qint64(42));
    }

    void activeSessionIgnoredWhenTooSmall()
    {
        ResumeInputs in;
        in.key = makeKey();
        in.activeSessionMatches = true;
        in.activeSessionPositionSec = 0.5; // below 1 s threshold
        in.storedEntry = makeStored(120.0, 3600.0);
        const auto r = resumeSecondsFor(in);
        QVERIFY(r.has_value());
        QCOMPARE(*r, qint64(120));
    }

    void storedFinishedReturnsNullopt()
    {
        ResumeInputs in;
        in.key = makeKey();
        in.storedEntry = makeStored(100.0, 200.0, /*finished*/ true);
        QVERIFY(!resumeSecondsFor(in).has_value());
    }

    void clampToDurationMinusFive()
    {
        ResumeInputs in;
        in.key = makeKey();
        in.storedEntry = makeStored(199.0, 200.0);
        const auto r = resumeSecondsFor(in);
        QVERIFY(r.has_value());
        QCOMPARE(*r, qint64(195));
    }

    void promptThreshold()
    {
        QVERIFY(shouldShowResumePrompt(120, 60));
        QVERIFY(!shouldShowResumePrompt(30, 60));
        QVERIFY(!shouldShowResumePrompt(0, 60));
    }
};

QTEST_MAIN(TstResumePolicy)
#include "tst_resume_policy.moc"
