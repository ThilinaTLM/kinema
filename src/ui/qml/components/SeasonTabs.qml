// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Season picker. Renders the `seriesDetailVm.seasonLabels`
// `QStringList` as a horizontal scrolling row of toggle buttons
// (Kirigami.NavigationTabBar would be the natural fit, but the
// Kirigami version shipped with Plasma 6 has alignment quirks under
// `ScrollablePage`; a plain `Repeater` of `Kirigami.Action`-driven
// buttons inside a `Flickable` is more portable).
//
// Keyboard navigation: Left/Right while focused walks the picker.
// Selecting a tab writes back to `vm.currentSeason`.
Item {
    id: tabs

    property var vm: seriesDetailVm

    implicitHeight: scroll.implicitHeight

    Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: row.implicitWidth
        contentHeight: row.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalFlick
        implicitHeight: row.implicitHeight + Kirigami.Units.smallSpacing * 2

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
                    onClicked: tabs.vm.currentSeason = index
                    KeyNavigation.left: tabs.vm.currentSeason > 0
                        ? row.children[index - 1] : null
                    KeyNavigation.right: tabs.vm.currentSeason
                            < tabs.vm.seasonLabels.length - 1
                        ? row.children[index + 1] : null
                }
            }
        }
    }
}
