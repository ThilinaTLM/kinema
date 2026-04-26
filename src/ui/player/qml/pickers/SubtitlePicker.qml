// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Subtitle picker. Includes a synthetic "Off" row at the top whose
 * selected state binds to `currentId === -1`.
 */
Popup {
    id: root
    property var model: null
    property int currentId: -1
    signal picked(int trackId)

    modal: true
    padding: 0
    width: 360
    height: Math.min(480, list.contentHeight + 100)

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
            text: qsTr("Subtitles")
            color: Theme.foreground
            font.pixelSize: Theme.fontSizeLg
            font.weight: Font.DemiBold
        }

        // Synthetic "Off" row.
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            color: offMouse.containsMouse
                ? Qt.rgba(1, 1, 1, 0.06) : "transparent"
            radius: 6
            Behavior on color { ColorAnimation { duration: Theme.fadeMs } }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacing
                Text {
                    Layout.preferredWidth: 18
                    text: root.currentId === -1 ? "\u2713" : ""
                    color: Theme.accent
                    font.pixelSize: Theme.fontSizeLg
                }
                Text {
                    Layout.fillWidth: true
                    text: qsTr("Off")
                    color: Theme.foreground
                    font.pixelSize: Theme.fontSize
                }
            }
            MouseArea {
                id: offMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: { root.picked(-1); root.close(); }
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.model
            spacing: 2

            delegate: Rectangle {
                width: list.width
                height: 44
                color: rowMouse.containsMouse
                    ? Qt.rgba(1, 1, 1, 0.06) : "transparent"
                radius: 6
                Behavior on color { ColorAnimation { duration: Theme.fadeMs } }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.spacing
                    anchors.rightMargin: Theme.spacing
                    Text {
                        Layout.preferredWidth: 18
                        text: root.currentId === trackId ? "\u2713" : ""
                        color: Theme.accent
                        font.pixelSize: Theme.fontSizeLg
                    }
                    Text {
                        Layout.fillWidth: true
                        text: forced ? title + " \u2014 " + qsTr("forced") : title
                        color: Theme.foreground
                        font.pixelSize: Theme.fontSize
                        elide: Text.ElideRight
                    }
                }
                MouseArea {
                    id: rowMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { root.picked(trackId); root.close(); }
                }
            }
        }
    }
}
