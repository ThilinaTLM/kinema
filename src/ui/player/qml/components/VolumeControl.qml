// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Compact volume control: speaker / mute toggle plus a horizontal
 * slider. Drag or click on the slider; click on the speaker to
 * toggle mute.
 */
Item {
    id: root
    property double volumePercent: 100
    property bool muted: false

    signal volumeChanged(double v)
    signal muteToggled()

    readonly property double _ratio:
        muted ? 0 : Math.max(0, Math.min(1, volumePercent / 100))

    RowLayout {
        anchors.fill: parent
        spacing: Theme.spacing

        // Mute toggle button
        Rectangle {
            Layout.preferredWidth: Theme.iconButton
            Layout.preferredHeight: Theme.iconButton
            radius: width / 2
            color: muteArea.containsMouse
                ? Theme.surfaceElev : "transparent"
            Behavior on color {
                ColorAnimation { duration: Theme.fadeMs }
            }
            IconGlyph {
                anchors.centerIn: parent
                kind: root.muted ? "mute" : "audio"
            }
            MouseArea {
                id: muteArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.muteToggled()
            }
        }

        // Slider
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.iconButton

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                height: 4
                radius: 2
                color: Theme.trackOff
            }
            Rectangle {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                height: 4
                radius: 2
                width: parent.width * root._ratio
                color: Theme.foreground
            }
            Rectangle {
                width: 12; height: 12; radius: 6
                color: Theme.foreground
                anchors.verticalCenter: parent.verticalCenter
                x: parent.width * root._ratio - 6
                visible: sliderArea.containsMouse || sliderArea.pressed
            }

            MouseArea {
                id: sliderArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onPressed: mouse => _set(mouse.x)
                onPositionChanged: mouse => {
                    if (pressed) _set(mouse.x);
                }
                onWheel: wheel => {
                    const step = wheel.angleDelta.y > 0 ? 5 : -5;
                    root.volumeChanged(
                        Math.max(0, Math.min(100, root.volumePercent + step)));
                    wheel.accepted = true;
                }
                function _set(x) {
                    const ratio = Math.max(0,
                        Math.min(1, x / Math.max(1, parent.width)));
                    root.volumeChanged(ratio * 100);
                }
            }
        }
    }
}
