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

        // ---- Layer 2: buffered span (dim base + tiled dots) ------
        // Clipped to the buffered ratio. The Canvas paints a 6 x 6
        // tile of small dots and tiles itself across the layer's
        // full width (cheap; only repaints on size change).
        Item {
            id: bufferedLayer
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: trackRow.bufferedWidth
            clip: true

            Rectangle {
                anchors.fill: parent
                color: Theme.trackBufferBg
                // Match parent radius on the left edge; right edge
                // is clipped flush by the parent Item, so radius on
                // the right doesn't matter.
                radius: trackRow.radius
            }

            Canvas {
                id: dotCanvas
                anchors.fill: parent
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    ctx.fillStyle = Theme.trackBufferDot;
                    const tile = 6;
                    const r = 1.1;
                    // Two-row staggered grid for a softer texture.
                    for (let y = tile / 2; y < height; y += tile) {
                        for (let x = (Math.floor(y / tile) % 2) ? 0 : tile / 2;
                             x < width; x += tile) {
                            ctx.beginPath();
                            ctx.arc(x, y, r, 0, Math.PI * 2);
                            ctx.fill();
                        }
                    }
                }
            }
        }

        // ---- Layer 3: played fill --------------------------------
        Rectangle {
            id: playedFill
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: trackRow.playedWidth
            radius: trackRow.radius
            color: Theme.trackPlayed
        }

        // ---- Chapter ticks ---------------------------------------
        Repeater {
            model: root.chapters
            delegate: Rectangle {
                visible: root.duration > 0 && time > 0
                    && time < root.duration
                width: 2
                height: trackRow.height
                color: Theme.foreground
                opacity: 0.65
                x: root.duration > 0
                    ? trackRow.width * (time / root.duration) - 1
                    : 0
            }
        }

        // ---- Internal time labels (dual-colour clipping) ---------
        // Two sibling clip-Items per label, each anchored to the
        // played boundary. The "on-played" copy uses
        // `trackTextOnPlayed` (dark over white); the "on-rest" copy
        // uses `trackTextOnRest` (light over dim). The text payload
        // is identical, geometry identical — only the clip rect and
        // colour differ. As `_ratio` moves, the boundary slides
        // and each side renders the visible portion.

        // Elapsed (left)
        Item {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: trackRow.playedWidth
            clip: true
            Text {
                id: elapsedTextOnPlayed
                anchors.left: parent.left
                anchors.leftMargin: Theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.trackTextOnPlayed
                font: Theme.tabularSmallFont
                text: _fmt(root.position)
            }
        }
        Item {
            anchors.left: parent.left
            anchors.leftMargin: trackRow.playedWidth
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            clip: true
            Text {
                anchors.left: parent.left
                // Compensate so the rest-side copy lines up with
                // the played-side copy (same screen position).
                anchors.leftMargin: Theme.spacingSm - trackRow.playedWidth
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.trackTextOnRest
                font: Theme.tabularSmallFont
                text: elapsedTextOnPlayed.text
            }
        }

        // Remaining (right)
        Item {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: trackRow.playedWidth
            clip: true
            Text {
                id: remainingTextOnPlayed
                anchors.right: parent.right
                anchors.rightMargin:
                    Theme.spacingSm + (trackRow.width - trackRow.playedWidth)
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.trackTextOnPlayed
                font: Theme.tabularSmallFont
                text: "-" + _fmt(Math.max(0, root.duration - root.position))
            }
        }
        Item {
            anchors.left: parent.left
            anchors.leftMargin: trackRow.playedWidth
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            clip: true
            Text {
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.trackTextOnRest
                font: Theme.tabularSmallFont
                text: remainingTextOnPlayed.text
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
