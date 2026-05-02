// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/PlayerWindowPresentation.h"

#include <QTest>

using namespace kinema::ui::player::window_presentation;

class TestPlayerWindowPresentation : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void hiddenWindowIsPresented();
    void visibleWindowIsNotRePresented();
};

void TestPlayerWindowPresentation::hiddenWindowIsPresented()
{
    const auto request = forPlayback(/*alreadyVisible=*/false);

    QVERIFY(request.showWindow);
    QVERIFY(request.raiseWindow);
    QVERIFY(request.requestActivation);
}

void TestPlayerWindowPresentation::visibleWindowIsNotRePresented()
{
    const auto request = forPlayback(/*alreadyVisible=*/true);

    QVERIFY(!request.showWindow);
    QVERIFY(!request.raiseWindow);
    QVERIFY(!request.requestActivation);
}

QTEST_GUILESS_MAIN(TestPlayerWindowPresentation)
#include "tst_player_window_presentation.moc"
