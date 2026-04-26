// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Hard-coded speed picker. Values mirror the previous Lua chrome.
 */
Popup {
    id: root
    property double currentSpeed: 1.0
    signal picked(double factor)

    modal: true
    padding: 0
    width: 280
    height: list.contentHeight + 60

    readonly property var values: [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]

    background: Rectangle {
        color: Theme.surfaceElev
        radius: Theme.radiusLg
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacing
        spacing: 2

        Text {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing
            text: qsTr("Playback speed")
            color: Theme.foreground
            font.pixelSize: Theme.fontSizeLg
            font.weight: Font.DemiBold
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.values
            spacing: 2

            delegate: Rectangle {
                width: list.width
                height: 40
                radius: 6
                color: rowMouse.containsMouse
                    ? Qt.rgba(1, 1, 1, 0.06) : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.fadeMs } }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.spacing
                    anchors.rightMargin: Theme.spacing
                    Text {
                        Layout.preferredWidth: 18
                        text: Math.abs(root.currentSpeed - modelData) < 0.01
                            ? "\u2713" : ""
                        color: Theme.accent
                        font.pixelSize: Theme.fontSizeLg
                    }
                    Text {
                        Layout.fillWidth: true
                        text: modelData + "x"
                        color: Theme.foreground
                        font.pixelSize: Theme.fontSize
                    }
                }
                MouseArea {
                    id: rowMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { root.picked(modelData); root.close(); }
                }
            }
        }
    }
}
