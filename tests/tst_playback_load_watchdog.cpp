// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/PlaybackLoadWatchdog.h"

#include <QSignalSpy>
#include <QTest>

#include <chrono>

using kinema::controllers::PlaybackLoadWatchdog;
using namespace std::chrono_literals;

class TestPlaybackLoadWatchdog : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void firesAfterTimeoutWhenNotStopped();
    void doesNotFireWhenStoppedFirst();
    void restartsCleanlyOnRearm();
};

void TestPlaybackLoadWatchdog::firesAfterTimeoutWhenNotStopped()
{
    PlaybackLoadWatchdog wd;
    wd.setTimeout(20ms);

    QSignalSpy spy(&wd, &PlaybackLoadWatchdog::timedOut);
    wd.start();
    QVERIFY(wd.isActive());

    QVERIFY(spy.wait(500));
    QCOMPARE(spy.count(), 1);
    QVERIFY(!wd.isActive());
}

void TestPlaybackLoadWatchdog::doesNotFireWhenStoppedFirst()
{
    PlaybackLoadWatchdog wd;
    wd.setTimeout(50ms);

    QSignalSpy spy(&wd, &PlaybackLoadWatchdog::timedOut);
    wd.start();
    wd.stop();
    QVERIFY(!wd.isActive());

    QTest::qWait(120);
    QCOMPARE(spy.count(), 0);
}

void TestPlaybackLoadWatchdog::restartsCleanlyOnRearm()
{
    // Two consecutive queue items each arm the watchdog. Each load
    // succeeding (i.e. stopping the timer) before the next start
    // must not leak a stale fire.
    PlaybackLoadWatchdog wd;
    wd.setTimeout(40ms);

    QSignalSpy spy(&wd, &PlaybackLoadWatchdog::timedOut);

    wd.start();
    wd.stop();              // first item loaded fine

    wd.start();             // second item starts loading
    QVERIFY(spy.wait(500)); // and never produces fileLoaded
    QCOMPARE(spy.count(), 1);
}

QTEST_MAIN(TestPlaybackLoadWatchdog)
#include "tst_playback_load_watchdog.moc"
