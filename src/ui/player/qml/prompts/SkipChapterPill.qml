// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Floating pill button to skip the current chapter (intro / outro /
 * credits). Sits above the transport bar, fades in and out.
 */
Item {
    id: root
    property string label: ""
    signal skipClicked()

    width: pill.width
    height: pill.height
    visible: opacity > 0
    opacity: visible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    Rectangle {
        id: pill
        width: pillText.implicitWidth + 28
        height: 36
        radius: height / 2
        color: pillArea.containsMouse ? Theme.accentBright : Theme.accent
        Behavior on color { ColorAnimation { duration: Theme.fadeMs } }

        Text {
            id: pillText
            anchors.centerIn: parent
            text: root.label
            color: "#0D0F12"
            font.pixelSize: Theme.fontSize
            font.weight: Font.DemiBold
        }

        MouseArea {
            id: pillArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.skipClicked()
        }
    }
}
