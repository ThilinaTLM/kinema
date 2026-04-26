// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Single row used by Audio / Subtitle / Speed pickers. Visual model:
 *
 *   - hover: 30 % accent fill + 1 px accent border, radius 5
 *   - selected: full accent fill, white (highlightedText) text,
 *     check glyph in highlightedText so it reads against the pill
 *   - resting: transparent
 *
 * Mirrors qqc2-desktop-style's `DefaultListItemBackground` /
 * `MenuItem` look.
 */
Item {
    id: row
    property string label: ""
    property string trailing: ""
    property bool selected: false
    // Mirrors the internal HoverHandler so callers can show their
    // own tooltips / opacity tweaks without re-implementing hover.
    readonly property bool hovered: hover.hovered
    signal clicked()

    implicitWidth: parent ? parent.width : 0
    implicitHeight: 36

    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: row.clicked()
    }

    Rectangle {
        anchors.fill: parent
        radius: Theme.radius
        border.width: 1
        color: row.selected
            ? Theme.selectedFill
            : (hover.hovered ? Theme.hoverFill : "transparent")
        border.color: row.selected
            ? Theme.selectedFill
            : (hover.hovered ? Theme.accent : "transparent")
        Behavior on color { ColorAnimation { duration: Theme.fadeMs } }
        Behavior on border.color { ColorAnimation { duration: Theme.fadeMs } }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing
        anchors.rightMargin: Theme.spacing
        spacing: Theme.spacing

        Label {
            Layout.preferredWidth: 14
            text: row.selected ? "\u2713" : ""
            color: Theme.highlightedText
            font.weight: Font.Bold
            horizontalAlignment: Text.AlignHCenter
        }
        Label {
            Layout.fillWidth: true
            text: row.label
            color: row.selected ? Theme.highlightedText : Theme.foreground
            elide: Text.ElideRight
        }
        Label {
            text: row.trailing
            color: row.selected ? Theme.highlightedText : Theme.foregroundDim
            font: Theme.smallFont
            visible: text.length > 0
        }
    }
}
