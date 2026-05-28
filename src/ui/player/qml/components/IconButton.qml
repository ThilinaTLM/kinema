// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
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
 *
 * Accessibility / keyboard: set `accessibleName` (an i18nc string) at
 * every call site. The button exposes itself as an `Accessible.Button`,
 * is reachable via Tab (`activeFocusOnTab`), activates on Return / Space,
 * paints a focus ring when focused, and surfaces `accessibleName` as a
 * hover `ToolTip` so the icon-only chrome is discoverable.
 */
Item {
    id: root
    property string iconKind: ""
    property bool checked: false
    property string accessibleName: ""
    readonly property bool hovered: hover.hovered
    signal clicked()

    implicitWidth: Theme.iconButton
    implicitHeight: Theme.iconButton

    activeFocusOnTab: enabled

    Accessible.role: Accessible.Button
    Accessible.name: root.accessibleName
    Accessible.focusable: true
    Accessible.onPressAction: root.activate()

    // Single activation path shared by tap, keyboard, and the
    // accessibility press action.
    function activate() {
        if (!root.enabled)
            return;
        root.clicked();
        pressFlash.start();
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter
                || event.key === Qt.Key_Space) {
            root.activate();
            event.accepted = true;
        }
    }

    QQC2.ToolTip.text: root.accessibleName
    QQC2.ToolTip.visible: hover.hovered && root.accessibleName.length > 0
    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay

    HoverHandler {
        id: hover
        enabled: root.enabled
        cursorShape: root.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
    }
    TapHandler {
        id: tap
        enabled: root.enabled
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: root.activate()
    }

    // Keyboard focus ring. Themed (no hard-coded color); only painted
    // while the button holds active focus from Tab navigation.
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.color: Theme.accent
        border.width: Math.max(1, Theme.unit / 2)
        visible: root.activeFocus
    }

    // Resting / hover / pressed background. Press flash sits on top
    // and animates its own opacity back to 0 regardless of state.
    Rectangle {
        id: bg
        anchors.fill: parent
        radius: width / 2
        color: {
            if (!root.enabled)  return "transparent";
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
        opacity: root.enabled ? 1.0 : 0.35
    }
}
