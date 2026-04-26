// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Horizontal row of compact chips ("4K", "HDR10", "EAC3 5.1", …).
 * Driven by `playerVm.mediaChips` (a QStringList). Each chip is
 * a small rounded rectangle with foreground text.
 */
Item {
    id: root
    property var chips: []
    implicitHeight: 22

    Row {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.unit

        Repeater {
            model: root.chips
            delegate: Rectangle {
                radius: 4
                color: Qt.rgba(1, 1, 1, 0.12)
                height: 22
                width: chipText.implicitWidth + 12

                Text {
                    id: chipText
                    anchors.centerIn: parent
                    text: modelData
                    color: Theme.foreground
                    font.pixelSize: Theme.fontSizeSm
                    font.weight: Font.Medium
                }
            }
        }
    }
}
