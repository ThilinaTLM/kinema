// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Thick seek bar with chapter ticks, cache-ahead pattern, hover
 * preview, drag-to-seek and **internal time labels** (elapsed
 * left, remaining right) overlaid on the bar itself.
 *
 * Visual idiom (Plex/Netflix):
 *
 *   ┌────────────────────────────────────────────────────────────┐
 *   │ played (solid white) │ buffered (dim + dotted) │ rest (dim)│
 *   │ 12:03                                              -42:29 │
 *   └────────────────────────────────────────────────────────────┘
 *
 * Time labels render twice with dual-colour clipping so they read
 * dark over the white played span and light over the dim rest,
 * regardless of where the playhead sits.
 *
 * Stateless: position / duration / cacheAhead / chapters in;
 * `seekRequested(seconds)` out. `thumbnailUrl` is a stub for a
 * future hover-thumbnail enhancement — empty for now, no rendering.
 */
Item {
    id: root
    property double position: 0.0
    property double duration: 0.0
    property double cacheAhead: 0.0
    property var chapters: null

    // Future hook: a thumbnail URL the hover tooltip can render
    // above the bar. Empty disables it.
    property string thumbnailUrl: ""

    signal seekRequested(double seconds)

    readonly property bool _hover: hoverArea.containsMouse || dragArea.pressed
    readonly property double _ratio:
        duration > 0 ? Math.max(0, Math.min(1, position / duration)) : 0
    readonly property double _bufferRatio:
        duration > 0
        ? Math.max(0, Math.min(1, (position + cacheAhead) / duration))
        : 0

    // Vertical breathing room above the bar so the hover tooltip
    // and a future thumbnail preview have somewhere to float.
    height: Theme.seekBarHeightThick + Theme.spacingSm

    // Track row vertically centred so the bar grows symmetrically.
    // Hit-target padding: the surrounding MouseAreas anchor to the
    // host root so the user can grab the bar from anywhere within
    // this Item's height, not just the visual rectangle.
    Item {
        id: trackRow
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: Theme.seekBarHeightThick

        readonly property real radius: Theme.radius
        readonly property real playedWidth: trackRow.width * root._ratio
        readonly property real bufferedWidth: trackRow.width * root._bufferRatio

        // ---- Layer 1: rest (un-buffered) -------------------------
        Rectangle {
            anchors.fill: parent
            radius: trackRow.radius
            color: Theme.trackRest
        }

        // ---- Layer 2: buffered span (rounded-left, square-right) -
        // Slightly less dim than the rest layer so the eye can
        // tell "loaded ahead" from "not yet loaded". The rounded
        // LEFT corners line up with the outer rest layer's curve;
        // the RIGHT edge is a straight vertical line because that
        // edge is a *progress boundary*, not the visual end of the
        // bar — a rounded corner there reads as a thumb tapering
        // off and confuses the eye about where buffered ends.
        //
        // Implementation trick (Qt 6.6 has no per-corner radius):
        // a wider rounded "cap" rectangle whose right half is
        // overpainted by a square "body" rectangle, leaving only
        // the rounded LEFT corners visible.
        Item {
            id: bufferedFill
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: trackRow.bufferedWidth
            visible: width > 0
            clip: true

            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: trackRow.radius * 2
                radius: trackRow.radius
                color: Theme.trackBufferBg
            }
            Rectangle {
                anchors.left: parent.left
                anchors.leftMargin: trackRow.radius
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                color: Theme.trackBufferBg
            }
        }

        // ---- Layer 3: played fill (rounded-left, square-right) --
        // Same idiom as the buffered layer above.
        Item {
            id: playedFill
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: trackRow.playedWidth
            visible: width > 0
            clip: true

            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: trackRow.radius * 2
                radius: trackRow.radius
                color: Theme.trackPlayed
            }
            Rectangle {
                anchors.left: parent.left
                anchors.leftMargin: trackRow.radius
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                color: Theme.trackPlayed
            }
        }

        // ---- Chapter ticks ---------------------------------------
        // Subtle by design — the bar already conveys progress; the
        // ticks are a navigational hint, not a primary visual.
        Repeater {
            model: root.chapters
            delegate: Rectangle {
                visible: root.duration > 0 && time > 0
                    && time < root.duration
                width: 2
                height: trackRow.height
                color: Theme.foreground
                opacity: 0.30
                x: root.duration > 0
                    ? trackRow.width * (time / root.duration) - 1
                    : 0
            }
        }

        // ---- Internal time labels --------------------------------
        // White text on a translucent dark pill so the labels read
        // clearly regardless of what's underneath (white played
        // fill, dim rest, chapter tick). The pill auto-sizes to
        // the text plus small padding; both labels use the
        // standard tabular font for a more substantial weight than
        // the previous small variant.
        Rectangle {
            id: elapsedPill
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacingSm
            anchors.verticalCenter: parent.verticalCenter
            radius: Theme.radius
            color: Theme.trackLabelBg
            width: elapsedText.implicitWidth + Theme.spacing
            height: elapsedText.implicitHeight + Theme.spacingXs
            Text {
                id: elapsedText
                anchors.centerIn: parent
                color: Theme.foreground
                font: Theme.tabularFont
                text: root._fmt(root.position)
            }
        }

        Rectangle {
            id: remainingPill
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacingSm
            anchors.verticalCenter: parent.verticalCenter
            radius: Theme.radius
            color: Theme.trackLabelBg
            width: remainingText.implicitWidth + Theme.spacing
            height: remainingText.implicitHeight + Theme.spacingXs
            Text {
                id: remainingText
                anchors.centerIn: parent
                color: Theme.foreground
                font: Theme.tabularFont
                text: "-" + root._fmt(
                    Math.max(0, root.duration - root.position))
            }
        }
    }

    // Time formatter shared by the labels and the hover tooltip.
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

    // Hover-tooltip preview (timestamp under the cursor)
    Rectangle {
        id: tooltip
        visible: hoverArea.containsMouse && root.duration > 0
        color: Theme.surfaceElev
        radius: Theme.radius
        height: tooltipText.implicitHeight + Theme.spacingSm
        width: tooltipText.implicitWidth + Theme.spacing
        x: Math.max(0, Math.min(root.width - width,
            hoverArea.mouseX - width / 2))
        y: -(height + Theme.spacingXs)
        border.color: Theme.border
        border.width: 1

        Text {
            id: tooltipText
            anchors.centerIn: parent
            color: Theme.foreground
            font: Theme.tabularSmallFont
            text: {
                if (root.duration <= 0) return "";
                const t = root.duration *
                    (hoverArea.mouseX / Math.max(1, root.width));
                return root._fmt(t);
            }
        }
    }

    // Hover detection (separate from press so the tooltip stays up
    // while not dragging).
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
