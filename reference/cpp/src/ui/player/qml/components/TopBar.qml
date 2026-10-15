// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Title strip across the top of the video. Auto-hides with the rest
 * of the chrome via `chromeVisible`. Title / subtitle bind directly
 * to PlayerViewModel; chips render under the subtitle when present.
 *
 * The previous close X has been removed — Esc and the window
 * manager's X cover that role.
 */
Rectangle {
    id: root
    height: Theme.topBarHeight
    color: "transparent"

    property bool chromeVisible: true

    opacity: chromeVisible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    // Stop intercepting hover / clicks once fully transparent so
    // invisible buttons don't catch the cursor on its way out.
    visible: opacity > 0.001

    // Gradient fade so text reads against bright frames without a
    // hard chrome bar shape competing with the video.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.rgba(0, 0, 0, 0.55) }
            GradientStop { position: 1.0; color: "transparent" }
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacingLg
        anchors.rightMargin: Theme.spacingLg
        spacing: Theme.spacing

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            Text {
                Layout.fillWidth: true
                text: playerVm.mediaTitle
                color: Theme.foreground
                font: Theme.titleFont
                elide: Text.ElideRight
                visible: text.length > 0
            }
            Text {
                Layout.fillWidth: true
                text: playerVm.mediaSubtitle
                color: Theme.foregroundDim
                font: Theme.smallFont
                elide: Text.ElideRight
                visible: text.length > 0
            }
            MediaChips {
                Layout.fillWidth: true
                chips: playerVm.mediaChips
                visible: chips.length > 0
            }
        }
    }
}
