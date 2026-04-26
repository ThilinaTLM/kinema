// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Templates as T
import dev.tlmtech.kinema.player

/**
 * Flat text button styled to match Breeze. Two flavours:
 *
 *   - `primary: true`  — full accent fill, highlightedText label
 *   - `primary: false` — transparent fill with 1 px border, accent
 *                        on hover (Breeze "neutral" button)
 *
 * Used by the resume prompt, next-episode banner, and anywhere else
 * we'd previously have reached for `Button { highlighted: true }`.
 * The stock `Button` styling depends on the loaded QtQuick.Controls
 * style and looks foreign over the dark video chrome.
 */
T.Button {
    id: control
    property bool primary: false

    implicitWidth: Math.max(96,
        contentItem.implicitWidth + leftPadding + rightPadding)
    implicitHeight: 32
    horizontalPadding: Theme.spacing + Theme.spacingXs
    verticalPadding: Theme.spacingXs

    contentItem: Text {
        text: control.text
        color: control.primary
            ? Theme.highlightedText
            : (control.hovered ? Theme.foreground : Theme.foreground)
        font.weight: control.primary ? Font.DemiBold : Font.Normal
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        radius: Theme.radius
        border.width: 1
        color: {
            if (control.primary) {
                if (control.pressed) return Theme.accentPressed;
                if (control.hovered) return Theme.accentHover;
                return Theme.accent;
            }
            if (control.pressed) return Theme.hoverFill;
            if (control.hovered) return Theme.hoverFill;
            return "transparent";
        }
        border.color: control.primary
            ? color
            : (control.hovered ? Theme.accent : Theme.border)
        Behavior on color { ColorAnimation { duration: Theme.fadeMs } }
        Behavior on border.color { ColorAnimation { duration: Theme.fadeMs } }
    }
}
