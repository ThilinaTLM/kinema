// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Modal-ish overlay listing the keyboard shortcuts. Closes on Esc,
 * `?`, or click anywhere outside the panel. Text comes
 * pre-translated from C++ via `playerVm.cheatSheetText`.
 */
Item {
    id: root
    property string text: ""
    signal closeRequested()

    width: parent ? parent.width : 0
    height: parent ? parent.height : 0
    visible: opacity > 0
    opacity: visible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    // Backdrop dim
    Rectangle {
        anchors.fill: parent
        color: Theme.overlayShade
        MouseArea {
            anchors.fill: parent
            onClicked: root.closeRequested()
        }
    }

    // Panel
    Rectangle {
        anchors.centerIn: parent
        width: Math.min(560, parent.width - 64)
        height: Math.min(480, parent.height - 64)
        radius: Theme.radiusLg
        color: Theme.surfaceElev

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Theme.spacingLg
            spacing: Theme.spacing

            Text {
                Layout.fillWidth: true
                text: qsTr("Keyboard shortcuts")
                color: Theme.foreground
                font.pixelSize: Theme.fontSizeLg
                font.weight: Font.DemiBold
            }

            Flickable {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: width
                contentHeight: contentText.implicitHeight
                clip: true

                Text {
                    id: contentText
                    width: parent.width
                    text: root.text
                    color: Theme.foregroundDim
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }
            }
        }

        // Close button (top-right)
        Rectangle {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Theme.spacing
            width: 32; height: 32; radius: 16
            color: closeArea.containsMouse
                ? Qt.rgba(1, 1, 1, 0.1) : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.fadeMs } }

            Text {
                anchors.centerIn: parent
                text: "\u2715" // ✕
                color: Theme.foreground
                font.pixelSize: 18
            }
            MouseArea {
                id: closeArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.closeRequested()
            }
        }
    }
}
