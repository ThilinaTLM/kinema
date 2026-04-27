// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Seek bar with chapter ticks, cache-ahead shading, hover preview
 * and drag-to-seek. Stateless: position and duration come in;
 * `seekRequested(seconds)` goes out. Hovering grows the bar's
 * height for a tiny affordance bump.
 */
Item {
    id: root
    property double position: 0.0
    property double duration: 0.0
    property double cacheAhead: 0.0
    property var chapters: null

    signal seekRequested(double seconds)

    readonly property bool _hover: hoverArea.containsMouse || dragArea.pressed
    readonly property double _ratio:
        duration > 0 ? Math.max(0, Math.min(1, position / duration)) : 0
    readonly property double _bufferRatio:
        duration > 0
        ? Math.max(0, Math.min(1, (position + cacheAhead) / duration))
        : 0

    height: Theme.seekBarHeightHover + Theme.spacingSm

    // Track row vertically centred so the bar grows symmetrically.
    //
    // Hit-target padding: even when the visual bar is 10 px tall,
    // the surrounding MouseAreas anchor to `parent` so the user can
    // grab the bar by clicking anywhere within this Item's height.

    Item {
        id: trackRow
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: root._hover ? Theme.seekBarHeightHover : Theme.seekBarHeight
        Behavior on height { NumberAnimation { duration: Theme.growMs } }

        // Background track
        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Theme.trackOff
        }
        // Buffered-ahead shade
        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: parent.width * root._bufferRatio
            radius: height / 2
            color: Theme.trackBuffer
        }
        // Played progress
        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: parent.width * root._ratio
            radius: height / 2
            color: Theme.accent
        }
        // Chapter ticks (skipped when chapters list is empty)
        Repeater {
            model: root.chapters
            delegate: Rectangle {
                visible: root.duration > 0 && time > 0
                    && time < root.duration
                width: 2
                height: trackRow.height + 4
                color: Theme.foreground
                opacity: 0.7
                y: -2
                x: root.duration > 0
                    ? trackRow.width * (time / root.duration) - 1
                    : 0
            }
        }
    }

    // Drag handle (grows on hover/drag)
    Rectangle {
        id: handle
        width: root._hover ? Theme.seekHandleSize : 0
        height: width
        radius: width / 2
        color: Theme.accentHover
        anchors.verticalCenter: trackRow.verticalCenter
        x: trackRow.width * root._ratio - width / 2
        Behavior on width { NumberAnimation { duration: Theme.growMs } }
    }

    // Hover-tooltip preview
    Rectangle {
        id: tooltip
        visible: hoverArea.containsMouse && root.duration > 0
        color: Theme.surfaceElev
        radius: 4
        height: 24
        width: tooltipText.implicitWidth + 12
        x: Math.max(0, Math.min(root.width - width,
            hoverArea.mouseX - width / 2))
        // Float clear of the (now thicker) bar; the tooltip lives
        // above the transport row regardless of where the bar sits.
        y: -(Theme.seekBarHeightHover + Theme.spacing + 24)

        Text {
            id: tooltipText
            anchors.centerIn: parent
            color: Theme.foreground
            font: Theme.tabularSmallFont
            text: {
                if (root.duration <= 0) return "";
                const t = root.duration *
                    (hoverArea.mouseX / Math.max(1, root.width));
                return _fmt(t);
            }
            function _fmt(t) {
                if (!isFinite(t) || t < 0) t = 0;
                const total = Math.floor(t);
                const h = Math.floor(total / 3600);
                const m = Math.floor((total % 3600) / 60);
                const s = total % 60;
                const pad = n => (n < 10 ? "0" + n : "" + n);
                return h > 0
                    ? h + ":" + pad(m) + ":" + pad(s)
                    : pad(m) + ":" + pad(s);
            }
        }
    }

    // Hover detection (separate from press to keep the tooltip while
    // not dragging).
    MouseArea {
        id: hoverArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
        cursorShape: Qt.PointingHandCursor
    }

    // Click / drag to seek.
    MouseArea {
        id: dragArea
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        onPressed: mouse => _seekTo(mouse.x)
        onPositionChanged: mouse => {
            if (pressed) _seekTo(mouse.x);
        }

        function _seekTo(x) {
            if (root.duration <= 0) return;
            const ratio = Math.max(0, Math.min(1, x / Math.max(1, root.width)));
            root.seekRequested(root.duration * ratio);
        }
    }
}
