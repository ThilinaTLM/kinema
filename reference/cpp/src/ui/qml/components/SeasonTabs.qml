// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Season picker. Renders the `seriesDetailVm.seasonLabels`
// `QStringList` as a horizontal scrolling row of toggle buttons.
// `Kirigami.NavigationTabBar` is documented for 3–5 actions with
// centred fixed-width segments — series with many seasons (Doctor
// Who, GoT, etc.) need a horizontally-scrollable strip, which the
// builtin doesn't provide. A `Repeater` of `Kirigami.Action`-driven
// buttons inside a `Flickable` is the right primitive here.
//
// Keyboard navigation: Left/Right while focused walks the picker.
// Selecting a tab writes back to `vm.currentSeason`.
Item {
    id: tabs

    property var vm: seriesDetailVm

    implicitHeight: scroll.implicitHeight

    // Right-click context menu. Per `docs/MenuConventions.md` the
    // "Mark watched" / "Mark unwatched" pair collapses into a
    // single stateful item that flips its label and icon based on
    // the season's current state. `targetWatched` is captured
    // alongside `targetSeason` when the user right-clicks a tab.
    KinemaMenu {
        id: seasonMenu
        property int targetSeason: -1
        property bool targetWatched: false

        KinemaMenuItem {
            iconName: seasonMenu.targetWatched
                ? "circle-dashed" : "circle-check"
            label: seasonMenu.targetWatched
                ? i18nc("@action:inmenu season tab", "Mark Unwatched")
                : i18nc("@action:inmenu season tab", "Mark Watched")
            onTriggered: tabs.vm.markSeasonWatched(
                seasonMenu.targetSeason,
                !seasonMenu.targetWatched)
        }
    }

    Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: row.implicitWidth
        contentHeight: row.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalFlick
        implicitHeight: row.implicitHeight

        Row {
            id: row
            spacing: Theme.inlineSpacing
            leftPadding: Theme.pageMargin
            rightPadding: Theme.pageMargin
            topPadding: Theme.inlineSpacing
            bottomPadding: Theme.inlineSpacing

            Repeater {
                model: tabs.vm.seasonLabels

                QQC2.Button {
                    text: modelData
                    checkable: true
                    checked: tabs.vm.currentSeason === index
                    flat: !checked
                    highlighted: checked
                    display: tabs.vm.seasonWatchedList[index] === true
                        ? QQC2.AbstractButton.TextBesideIcon
                        : QQC2.AbstractButton.TextOnly
                    icon.source: AppIcons.url("circle-check")
                    icon.color: Theme.positive
                    onClicked: tabs.vm.currentSeason = index
                    KeyNavigation.left: index > 0
                        ? (row.children[index - 1] ?? null) : null
                    KeyNavigation.right: index < tabs.vm.seasonLabels.length - 1
                        ? (row.children[index + 1] ?? null) : null

                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: {
                            const seasonNum = tabs.vm.seasonNumbers[index];
                            if (seasonNum !== undefined) {
                                seasonMenu.targetSeason = seasonNum;
                                seasonMenu.targetWatched =
                                    tabs.vm.seasonWatchedList[index] === true;
                                seasonMenu.popup();
                            }
                        }
                    }
                }
            }
        }
    }
}
