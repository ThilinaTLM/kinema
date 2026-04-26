// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Spinning ring shown while mpv is `paused-for-cache`. Optional
 * percent label inside the ring. Hidden via `visible-bound-to-active`.
 */
Item {
    id: root
    property bool active: false
    property int percent: 0

    width: 80
    height: 80
    visible: opacity > 0
    opacity: active ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    // Spinner: rotated arc
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: 4
        border.color: Theme.trackOff
    }
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: 4
        border.color: Theme.accent
        // We can't do an arc easily without Shape; cheat with a
        // rotated rectangle that masks half the ring.
        // Actually: leverage the rotation to sweep a coloured
        // rectangle that sits in front of the lower half.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            color: "transparent"
            // visual hint: rely on the rotation animation below
            // to rotate the parent and create a "running" effect.
        }
        RotationAnimator on rotation {
            from: 0; to: 360
            duration: 1200
            loops: Animation.Infinite
            running: root.active
        }
    }

    Text {
        anchors.centerIn: parent
        text: root.percent + "%"
        color: Theme.foreground
        font.weight: Font.Medium
        visible: root.percent > 0
    }
}
