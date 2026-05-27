// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/WatchedPolicy.h"

#include <QTest>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::policy;

class TstWatchedPolicy : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void defaultsAreCorrect()
    {
        QCOMPARE(defaultStopThreshold(domain::MediaKind::Movie), 0.85);
        QCOMPARE(defaultStopThreshold(domain::MediaKind::Series), 0.90);
    }

    void errorNeverFinishes()
    {
        WatchedInputs in;
        in.reason = PlaybackEndReason::PlayerError;
        in.positionSec = 999.0;
        in.durationSec = 1000.0;
        const auto d = decideWatched(in);
        QVERIFY(!d.finished);
    }

    void naturalEofAlwaysFinishes()
    {
        WatchedInputs in;
        in.reason = PlaybackEndReason::NaturalEof;
        in.positionSec = 10.0;
        in.durationSec = 1000.0;
        QVERIFY(decideWatched(in).finished);
    }

    void userStopBelowThreshold()
    {
        WatchedInputs in;
        in.reason = PlaybackEndReason::UserStop;
        in.positionSec = 500.0;
        in.durationSec = 1000.0;
        in.stopThreshold = 0.85;
        QVERIFY(!decideWatched(in).finished);
    }

    void userStopAboveThreshold()
    {
        WatchedInputs in;
        in.reason = PlaybackEndReason::UserStop;
        in.positionSec = 900.0;
        in.durationSec = 1000.0;
        in.stopThreshold = 0.85;
        QVERIFY(decideWatched(in).finished);
    }

    void creditsStartTakesPrecedence()
    {
        WatchedInputs in;
        in.reason = PlaybackEndReason::UserStop;
        in.positionSec = 550.0; // below 85% but past credits-2
        in.durationSec = 1000.0;
        in.creditsStartSec = 540.0;
        in.stopThreshold = 0.85;
        QVERIFY(decideWatched(in).finished);
    }

    void callerHintHonoured()
    {
        WatchedInputs in;
        in.reason = PlaybackEndReason::UserStop;
        in.positionSec = 1.0;
        in.durationSec = 1000.0;
        in.callerFinishedHint = true;
        QVERIFY(decideWatched(in).finished);
    }
};

QTEST_MAIN(TstWatchedPolicy)
#include "tst_watched_policy.moc"
