// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/PlayerViewModel.h"

#include <QSignalSpy>
#include <QTest>

using kinema::ui::player::PlayerViewModel;

class TestPlayerViewModel : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void resumeShowHide();
    void resumeAcceptSignal();
    void skipPillLifecycle();
    void mediaContextEmitsOnce();
    void chipsListChange();
    void loadingState();
    void queueNavigationState();
    void previousNextSignals();
    void noActionWhenAlreadyVisible();
};

void TestPlayerViewModel::resumeShowHide()
{
    PlayerViewModel vm;
    QVERIFY(!vm.resumeVisible());
    QCOMPARE(vm.resumeSeconds(), qint64 {0});

    QSignalSpy visSpy(&vm, &PlayerViewModel::resumeVisibleChanged);
    QSignalSpy secSpy(&vm, &PlayerViewModel::resumeSecondsChanged);

    vm.showResume(123);
    QVERIFY(vm.resumeVisible());
    QCOMPARE(vm.resumeSeconds(), qint64 {123});
    QCOMPARE(visSpy.count(), 1);
    QCOMPARE(secSpy.count(), 1);

    vm.hideResume();
    QVERIFY(!vm.resumeVisible());
    QCOMPARE(visSpy.count(), 2);
}

void TestPlayerViewModel::resumeAcceptSignal()
{
    PlayerViewModel vm;
    QSignalSpy acceptSpy(&vm, &PlayerViewModel::resumeAccepted);
    QSignalSpy declineSpy(&vm, &PlayerViewModel::resumeDeclined);

    vm.requestResumeAccept();
    QCOMPARE(acceptSpy.count(), 1);
    QCOMPARE(declineSpy.count(), 0);

    vm.requestResumeDecline();
    QCOMPARE(declineSpy.count(), 1);
}

void TestPlayerViewModel::skipPillLifecycle()
{
    PlayerViewModel vm;
    QSignalSpy visSpy(&vm, &PlayerViewModel::skipVisibleChanged);
    QSignalSpy reqSpy(&vm, &PlayerViewModel::skipRequested);

    vm.showSkip(QStringLiteral("intro"),
        QStringLiteral("Skip intro"), 30, 90);
    QVERIFY(vm.skipVisible());
    QCOMPARE(vm.skipKind(), QStringLiteral("intro"));
    QCOMPARE(vm.skipStartSec(), qint64 {30});
    QCOMPARE(vm.skipEndSec(), qint64 {90});
    QCOMPARE(visSpy.count(), 1);

    vm.requestSkip();
    QCOMPARE(reqSpy.count(), 1);

    vm.hideSkip();
    QVERIFY(!vm.skipVisible());
}

void TestPlayerViewModel::mediaContextEmitsOnce()
{
    PlayerViewModel vm;
    QSignalSpy titleSpy(&vm, &PlayerViewModel::mediaTitleChanged);

    vm.setMediaContext(QStringLiteral("Movie"),
        QStringLiteral("subtitle"),
        QStringLiteral("movie"));
    QCOMPARE(titleSpy.count(), 1);

    // Same context \u2192 zero re-emissions.
    vm.setMediaContext(QStringLiteral("Movie"),
        QStringLiteral("subtitle"),
        QStringLiteral("movie"));
    QCOMPARE(titleSpy.count(), 1);
}

void TestPlayerViewModel::chipsListChange()
{
    PlayerViewModel vm;
    QSignalSpy spy(&vm, &PlayerViewModel::mediaChipsChanged);

    vm.setMediaChips({ QStringLiteral("4K"),
        QStringLiteral("HDR10") });
    QCOMPARE(vm.mediaChips().size(), 2);
    QCOMPARE(spy.count(), 1);

    // Same list \u2192 no signal.
    vm.setMediaChips({ QStringLiteral("4K"),
        QStringLiteral("HDR10") });
    QCOMPARE(spy.count(), 1);
}

void TestPlayerViewModel::loadingState()
{
    PlayerViewModel vm;
    QSignalSpy spy(&vm, &PlayerViewModel::loadingVisibleChanged);

    QVERIFY(!vm.loadingVisible());
    vm.setLoadingVisible(true);
    QVERIFY(vm.loadingVisible());
    QCOMPARE(spy.count(), 1);

    vm.setLoadingVisible(true);
    QCOMPARE(spy.count(), 1);

    vm.setLoadingVisible(false);
    QVERIFY(!vm.loadingVisible());
    QCOMPARE(spy.count(), 2);
}

void TestPlayerViewModel::queueNavigationState()
{
    PlayerViewModel vm;
    QSignalSpy visSpy(&vm, &PlayerViewModel::queueNavigationVisibleChanged);
    QSignalSpy prevSpy(&vm, &PlayerViewModel::canGoPreviousChanged);
    QSignalSpy nextSpy(&vm, &PlayerViewModel::canGoNextChanged);

    vm.setQueueNavigationState(0, 3);
    QVERIFY(vm.queueNavigationVisible());
    QVERIFY(!vm.canGoPrevious());
    QVERIFY(vm.canGoNext());
    QCOMPARE(visSpy.count(), 1);
    QCOMPARE(prevSpy.count(), 0);
    QCOMPARE(nextSpy.count(), 1);

    vm.setQueueNavigationState(1, 3);
    QVERIFY(vm.canGoPrevious());
    QVERIFY(vm.canGoNext());
    QCOMPARE(prevSpy.count(), 1);

    vm.setQueueNavigationState(2, 3);
    QVERIFY(vm.canGoPrevious());
    QVERIFY(!vm.canGoNext());
    QCOMPARE(nextSpy.count(), 2);

    vm.setQueueNavigationState(-1, 0);
    QVERIFY(!vm.queueNavigationVisible());
    QVERIFY(!vm.canGoPrevious());
    QVERIFY(!vm.canGoNext());
    QCOMPARE(visSpy.count(), 2);
}

void TestPlayerViewModel::previousNextSignals()
{
    PlayerViewModel vm;
    QSignalSpy prevSpy(&vm, &PlayerViewModel::previousRequested);
    QSignalSpy nextSpy(&vm, &PlayerViewModel::nextRequested);

    vm.requestPrevious();
    vm.requestNext();

    QCOMPARE(prevSpy.count(), 1);
    QCOMPARE(nextSpy.count(), 1);
}

void TestPlayerViewModel::noActionWhenAlreadyVisible()
{
    PlayerViewModel vm;
    QSignalSpy spy(&vm, &PlayerViewModel::resumeVisibleChanged);
    vm.showResume(10);
    vm.showResume(10);
    QCOMPARE(spy.count(), 1);
}

QTEST_MAIN(TestPlayerViewModel)
#include "tst_player_view_model.moc"
