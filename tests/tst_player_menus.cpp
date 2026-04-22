// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MpvTrackList.h"
#include "ui/player/widgets/PlayerMenus.h"

#include <QAction>
#include <QMenu>
#include <QTest>

using namespace kinema::ui::player::widgets::menus;
using kinema::core::tracks::Entry;
using kinema::core::tracks::TrackList;

namespace {

Entry audio(int id, const QString& lang, bool selected = false,
    bool isDefault = false)
{
    Entry e;
    e.id = id;
    e.type = QStringLiteral("audio");
    e.lang = lang;
    e.codec = QStringLiteral("ac3");
    e.channelCount = 2;
    e.selected = selected;
    e.isDefault = isDefault;
    return e;
}

Entry sub(int id, const QString& lang, bool selected = false,
    bool forced = false)
{
    Entry e;
    e.id = id;
    e.type = QStringLiteral("sub");
    e.lang = lang;
    e.codec = QStringLiteral("subrip");
    e.selected = selected;
    e.forced = forced;
    return e;
}

} // namespace

class TestPlayerMenus : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    // --- Audio --------------------------------------------------------

    void populatesAudioMenuWithCheckmark()
    {
        QMenu menu;
        TrackList tracks = {
            audio(1, QStringLiteral("eng"), true, true),
            audio(2, QStringLiteral("jpn")),
        };

        int selectedId = 0;
        populateAudioMenu(&menu, tracks, [&](int id) {
            selectedId = id;
        });

        const auto actions = menu.actions();
        QCOMPARE(actions.size(), 2);
        QVERIFY(actions[0]->isCheckable());
        QVERIFY(actions[0]->isChecked());
        QVERIFY(!actions[1]->isChecked());

        actions[1]->trigger();
        QCOMPARE(selectedId, 2);
    }

    void emptyAudioMenuShowsDisabledPlaceholder()
    {
        QMenu menu;
        populateAudioMenu(&menu, TrackList {},
            [](int) { QFAIL("should not be invoked"); });

        const auto actions = menu.actions();
        QCOMPARE(actions.size(), 1);
        QVERIFY(!actions.first()->isEnabled());
    }

    // A second populate call must replace, not append.
    void populatesAudioMenuIdempotent()
    {
        QMenu menu;
        populateAudioMenu(&menu,
            { audio(1, QStringLiteral("eng")) },
            [](int) {});
        populateAudioMenu(&menu,
            { audio(2, QStringLiteral("fre")) },
            [](int) {});
        QCOMPARE(menu.actions().size(), 1);
    }

    // --- Subtitles ----------------------------------------------------

    void populatesSubtitleMenuWithNoneOption()
    {
        QMenu menu;
        TrackList tracks = {
            sub(1, QStringLiteral("eng"), true),
            sub(2, QStringLiteral("eng"), false, /*forced=*/true),
        };

        int lastId = -999;
        populateSubtitleMenu(&menu, tracks, [&](int id) {
            lastId = id;
        });

        const auto actions = menu.actions();
        // "None" + separator + 2 subtitle tracks.
        QCOMPARE(actions.size(), 4);
        // First item is the "None" entry; not checked (a sub is
        // selected).
        QVERIFY(actions[0]->isCheckable());
        QVERIFY(!actions[0]->isChecked());

        actions[0]->trigger();
        QCOMPARE(lastId, -1);

        actions[3]->trigger();
        QCOMPARE(lastId, 2);
    }

    // When no subtitle is selected, "None" is check-marked.
    void subtitleNoneCheckedWhenNoneSelected()
    {
        QMenu menu;
        TrackList tracks = {
            sub(1, QStringLiteral("eng")),
        };
        populateSubtitleMenu(&menu, tracks, [](int) {});
        QVERIFY(menu.actions()[0]->isChecked());
    }

    void subtitleMenuWithoutTracksShowsOnlyNone()
    {
        QMenu menu;
        populateSubtitleMenu(&menu, TrackList {}, [](int) {});
        // No tracks → no separator, just "None".
        QCOMPARE(menu.actions().size(), 1);
        QVERIFY(menu.actions()[0]->isChecked());
    }

    // --- Speed --------------------------------------------------------

    void speedMenuHasFixedLadder()
    {
        QMenu menu;
        populateSpeedMenu(&menu, /*current=*/1.0, [](double) {});
        QCOMPARE(menu.actions().size(), 6);
    }

    void speedMenuChecksCurrent()
    {
        QMenu menu;
        double chosen = 0.0;
        populateSpeedMenu(&menu, /*current=*/1.5,
            [&](double s) { chosen = s; });

        const auto actions = menu.actions();
        int checkedIdx = -1;
        for (int i = 0; i < actions.size(); ++i) {
            if (actions[i]->isChecked()) {
                checkedIdx = i;
            }
        }
        // 1.5× is the fifth in {0.5, 0.75, 1.0, 1.25, 1.5, 2.0}.
        QCOMPARE(checkedIdx, 4);

        actions[2]->trigger(); // 1.0
        QCOMPARE(chosen, 1.0);
    }

    // A speed that doesn't match the ladder exactly leaves nothing
    // checked; triggering any entry still reports the ladder speed.
    void speedMenuNoMatchNothingChecked()
    {
        QMenu menu;
        populateSpeedMenu(&menu, /*current=*/1.17, [](double) {});
        for (auto* a : menu.actions()) {
            QVERIFY(!a->isChecked());
        }
    }
};

QTEST_MAIN(TestPlayerMenus)
#include "tst_player_menus.moc"
