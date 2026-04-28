// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Always-visible vertical volume bar. Same visual idiom as
 * `SeekBar`, rotated 90°:
 *
 *   ┌───┐
 *   │░░░│  rest (above the played fill)
 *   │░░░│
 *   │┄┄┄│  dotted handle line at the played boundary
 *   │▓▓▓│  played fill anchored to bottom (solid white)
 *   │ 75│  centred volume number, dual-colour clipped
 *   │▓▓▓│
 *   └───┘
 *
 * Drag, click and mouse-wheel scrub the value. Mute is owned by an
 * external speaker `IconButton` (sits below this widget in the
 * scene); when `muted` is true the played fill collapses to zero
 * regardless of `volumePercent` so the visual matches what the
 * user actually hears.
 *
 * Stateless: `volumePercent` + `muted` in, `volumeChanged(v)` out.
 */
Item {
    id: root
    property double volumePercent: 100
    property bool muted: false

    signal volumeChanged(double v)

    readonly property double _ratio:
        muted ? 0 : Math.max(0, Math.min(1, volumePercent / 100))

    implicitWidth: Theme.volumeBarWidth
    implicitHeight: Theme.volumeBarHeight

    readonly property real _radius: Theme.radius
    readonly property real _playedHeight: height * _ratio

    // ---- Layer 1: rest --------------------------------------------
    Rectangle {
        anchors.fill: parent
        radius: root._radius
        color: Theme.trackRest
    }

    // ---- Layer 2: played (anchored to bottom) ---------------------
    Rectangle {
        id: playedFill
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: root._playedHeight
        radius: root._radius
        color: Theme.trackPlayed
    }

    // ---- Dotted handle line at the played boundary ----------------
    // Repeater of small white squares stamped horizontally across
    // the bar at the played/rest border. Hidden when the played
    // fill is zero (muted or 0%) so we don't sit a stray dashed
    // line at the bottom of an empty bar.
    Item {
        id: handleRow
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Theme.spacingSm
        anchors.rightMargin: Theme.spacingSm
        height: 2
        y: parent.height - root._playedHeight - height / 2
        visible: root._playedHeight > 0.5

        Row {
            anchors.fill: parent
            spacing: 2
            Repeater {
                model: Math.max(1, Math.floor(handleRow.width / 4))
                delegate: Rectangle {
                    width: 2
                    height: 2
                    color: Theme.trackBufferDot
                }
            }
        }
    }

    // ---- Internal volume number (dual-colour clipping) ------------
    // Same trick as SeekBar's time labels: render the same Text
    // twice, each clipped to one half of the boundary, so the
    // number reads dark over the white played fill and light over
    // the dim rest.
    Item {
        id: textOnRest
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: parent.height - root._playedHeight
        clip: true
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            // Anchor to the bar's centre, not this clip Item's, so
            // the value text stays centred regardless of which
            // side currently owns more of the bar.
            y: root.height / 2 - height / 2
            color: Theme.trackTextOnRest
            font: Theme.tabularSmallFont
            text: Math.round(root.volumePercent)
        }
    }
    Item {
        id: textOnPlayed
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: root._playedHeight
        clip: true
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            y: root.height / 2 - height / 2 - (root.height - root._playedHeight)
            color: Theme.trackTextOnPlayed
            font: Theme.tabularSmallFont
            text: Math.round(root.volumePercent)
        }
    }

    // ---- Pointer surface ------------------------------------------
    MouseArea {
        id: sliderArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onPressed: mouse => _set(mouse.y)
        onPositionChanged: mouse => {
            if (pressed) _set(mouse.y);
        }
        onWheel: wheel => {
            const step = wheel.angleDelta.y > 0 ? 5 : -5;
            root.volumeChanged(
                Math.max(0, Math.min(100, root.volumePercent + step)));
            wheel.accepted = true;
        }
        function _set(y) {
            // Invert: y=0 is top (=100 %), y=height is bottom (=0 %).
            const ratio = 1 - Math.max(0,
                Math.min(1, y / Math.max(1, root.height)));
            root.volumeChanged(ratio * 100);
        }
    }
}
