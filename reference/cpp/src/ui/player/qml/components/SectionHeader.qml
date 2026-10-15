// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Small all-caps label + horizontal rule. Used inside the info
 * overlay to chunk the keyboard shortcuts and stream-info sections
 * the way Plasma's System Settings does — visually distinct without
 * a heavy heading typeface.
 */
RowLayout {
    id: root
    property string text: ""
    spacing: Theme.spacing

    Label {
        text: root.text
        color: Theme.foregroundDim
        font.family: Theme.smallFont.family
        font.pointSize: Theme.smallFont.pointSize
        font.weight: Font.Bold
        font.capitalization: Font.AllUppercase
        font.letterSpacing: 0.6
    }
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 1
        color: Theme.border
    }
}
