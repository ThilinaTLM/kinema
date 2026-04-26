// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Slider-only volume control. Drag or click to set, scroll wheel
 * to nudge by 5 %.
 *
 * The mute / volume-state speaker icon used to live inside this
 * component, but it duplicated the audio-track picker speaker that
 * sat right next to it in the transport row. The mute toggle now
 * lives on the transport bar as a standalone `IconButton` whose
 * glyph derives from `volumePercent` + `muted`. This component
 * keeps `muted` as input + the (now unused inside this file)
 * `muteToggled` signal so the parent's wiring stays uniform; the
 * signal is harmless to leave defined.
 */
Item {
    id: root
    property double volumePercent: 100
    property bool muted: false

    signal volumeChanged(double v)
    signal muteToggled()

    readonly property double _ratio:
        muted ? 0 : Math.max(0, Math.min(1, volumePercent / 100))

    implicitWidth: Theme.volumeSliderWidth
    implicitHeight: Theme.iconButton

    // Track
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: 4
        radius: 2
        color: Theme.trackOff
    }
    // Filled progress
    Rectangle {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        height: 4
        radius: 2
        width: parent.width * root._ratio
        color: Theme.foreground
    }
    // Handle
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
                Math.min(1, x / Math.max(1, root.width)));
            root.volumeChanged(ratio * 100);
        }
    }
}
