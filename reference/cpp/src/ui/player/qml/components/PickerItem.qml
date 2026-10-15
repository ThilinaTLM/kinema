// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import dev.tlmtech.kinema.player

/**
 * Single row used by Audio / Subtitle / Speed pickers. Built on top
 * of the stock `QQC2.ItemDelegate` so it picks up Breeze's hover /
 * pressed states via the `org.kde.desktop` style configured in
 * `main.cpp`. The visuals (accent fill on selected, hover ring,
 * resting transparent) are kept consistent with the previous custom
 * implementation by overriding `background:` — the stock Breeze
 * background uses the View palette and would not match the dark
 * Header-colorSet popup surface around it.
 *
 * The selected indicator switches from a literal U+2713 character
 * to the system `checkmark` icon (`Kirigami.Icon`), which adapts to
 * the user's icon theme.
 *
 * Public API mirrors the previous version:
 *   - `label`     row text
 *   - `trailing`  small trailing caption (e.g. "Normal" on speed)
 *   - `selected`  drives accent fill + check
 *   - `hovered`   forwarded so callers can show their own tooltips
 *   - `clicked()` re-emitted from the delegate
 */
QQC2.ItemDelegate {
    id: row
    property string label: ""
    property string trailing: ""
    property bool selected: false

    // Stock ItemDelegate already exposes `hovered`, `pressed`, and the
    // `clicked()` signal — call sites use them directly.

    implicitWidth: parent ? parent.width : 0
    // Explicit height keeps the row tall enough for descenders
    // ('g', 'p', 'y' …) on every Plasma font setting. Without this
    // Breeze's stock ItemDelegate minimum (`gridUnit * 2`) is tight
    // enough that the contentItem can clip the bottom of the label.
    implicitHeight: Kirigami.Units.gridUnit * 2
        + Kirigami.Units.smallSpacing
    horizontalPadding: Theme.spacing
    verticalPadding: Theme.spacingXs
    highlighted: row.selected

    contentItem: RowLayout {
        spacing: Theme.spacing

        // Selected check or equal-width spacer so labels stay aligned
        // between selected and unselected rows.
        Item {
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: Kirigami.Units.iconSizes.small

            Kirigami.Icon {
                anchors.fill: parent
                visible: row.selected
                source: "checkmark"
                color: Kirigami.Theme.highlightedTextColor
                isMask: true
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            text: row.label
            color: row.selected
                ? Kirigami.Theme.highlightedTextColor
                : Kirigami.Theme.textColor
            elide: Text.ElideRight
        }
        QQC2.Label {
            text: row.trailing
            visible: text.length > 0
            color: row.selected
                ? Kirigami.Theme.highlightedTextColor
                : Kirigami.Theme.disabledTextColor
            font: Theme.smallFont
        }
    }

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: row.selected
            ? Kirigami.Theme.highlightColor
            : (row.hovered ? Theme.hoverFill : "transparent")
        border.width: row.selected || row.hovered ? 1 : 0
        border.color: row.selected
            ? Kirigami.Theme.highlightColor
            : (row.hovered ? Kirigami.Theme.highlightColor : "transparent")
        Behavior on color {
            ColorAnimation { duration: Theme.fadeMs }
        }
        Behavior on border.color {
            ColorAnimation { duration: Theme.fadeMs }
        }
    }
}
