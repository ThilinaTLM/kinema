// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Round chrome button. Used by TopBar, TransportBar, PopupPanel
 * and anywhere else in the player chrome that needs a tappable
 * icon.
 *
 * Two interaction details that the previous inline button got wrong:
 *
 *   1. Hover via `HoverHandler` (not `MouseArea.containsMouse`).
 *      A modal `Popup` opening on click intercepts pointer events
 *      without delivering an `exited` event to the underlying
 *      `MouseArea`, so `containsMouse` stayed `true` and the button
 *      kept its hover background after the popup closed.
 *      `HoverHandler.hovered` correctly tracks geometric occupancy.
 *
 *   2. A press flash that decays independently of hover, so even
 *      when the cursor is parked over Play/Pause the user sees a
 *      "I clicked it" pulse instead of nothing changing.
 *
 * `checked` paints the resting state with the soft accent fill —
 * meant for toggle-state buttons.
 */
Item {
    id: root
    property string iconKind: ""
    property bool checked: false
    signal clicked()

    implicitWidth: Theme.iconButton
    implicitHeight: Theme.iconButton

    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        id: tap
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: {
            root.clicked();
            pressFlash.start();
        }
    }

    // Resting / hover / pressed background. Press flash sits on top
    // and animates its own opacity back to 0 regardless of state.
    Rectangle {
        id: bg
        anchors.fill: parent
        radius: width / 2
        color: {
            if (root.checked)   return Theme.hoverFill;
            if (tap.pressed)    return Theme.hoverFill;
            if (hover.hovered)  return Qt.rgba(1, 1, 1, 0.08);
            return "transparent";
        }
        Behavior on color { ColorAnimation { duration: Theme.fadeMs } }

        Rectangle {
            id: flash
            anchors.fill: parent
            radius: parent.radius
            color: Theme.accent
            opacity: 0
            SequentialAnimation on opacity {
                id: pressFlash
                running: false
                NumberAnimation { from: 0;    to: 0.45; duration: 80 }
                NumberAnimation { from: 0.45; to: 0;    duration: 220 }
            }
        }
    }

    IconGlyph {
        anchors.centerIn: parent
        kind: root.iconKind
        color: root.checked ? Theme.highlightedText : Theme.foreground
    }
}
