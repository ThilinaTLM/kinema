// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Transient vertical volume HUD shown for ~1.5 s after a volume
 * change. Driven by an external `flash()` call so the parent can
 * decide which signal triggers the show (volume vs. mute).
 */
Rectangle {
    id: root
    property double volumePercent: 100
    property bool muted: false

    width: 36
    height: 160
    radius: Theme.radius
    color: Theme.surfaceElev

    opacity: 0.0
    visible: opacity > 0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    function flash() {
        opacity = 0.95;
        hideTimer.restart();
    }

    Timer {
        id: hideTimer
        interval: 1500
        onTriggered: root.opacity = 0.0
    }

    Item {
        anchors.fill: parent
        anchors.margins: 8

        // Vertical track
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 18
            anchors.top: parent.top
            anchors.topMargin: 4
            width: 4
            radius: 2
            color: Theme.trackOff
        }
        // Filled portion
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 18
            width: 4
            radius: 2
            height: root.muted
                ? 0
                : (parent.height - 22)
                  * Math.max(0, Math.min(1, root.volumePercent / 100))
            color: Theme.accent
        }
        Text {
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            color: Theme.foreground
            font: Theme.smallFont
            text: root.muted ? "M" : Math.round(root.volumePercent)
        }
    }
}
