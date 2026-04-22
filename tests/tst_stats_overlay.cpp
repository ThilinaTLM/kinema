// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/widgets/StatsOverlay.h"

#include <QTest>

using namespace kinema::ui::player::widgets;

class TestStatsOverlay : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    // Regression: the overlay starts hidden, but hide() sends a
    // synchronous hideEvent() during construction. That must stay safe
    // even before any later show/hide cycles.
    void constructorAndVisibilityCycleAreSafe()
    {
        StatsOverlay overlay;
        QVERIFY(overlay.isHidden());

        overlay.show();
        QVERIFY(!overlay.isHidden());

        overlay.hide();
        QVERIFY(overlay.isHidden());
    }
};

QTEST_MAIN(TestStatsOverlay)
#include "tst_stats_overlay.moc"
