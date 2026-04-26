// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Shared chrome for the player's overlay surfaces (info panel,
 * pickers, prompts). Mirrors the KDE Plasma Breeze popup look:
 *
 *   - radius `Theme.radiusLg` (8 px)
 *   - 1 px `Theme.border` (≈ Plasma's frameContrast tint)
 *   - drop shadow via `MultiEffect` (Qt 6.5+ stock)
 *   - header strip with title + optional close pill, separated from
 *     the content by a 1 px rule
 *
 * Children placed inside `PopupPanel { … }` go into the body area
 * via the default `contentItem` alias.
 *
 * `closable` controls whether the close pill is shown — pickers
 * close themselves on selection, so they hide it; the info overlay
 * shows it.
 */
Item {
    id: root
    property string title: ""
    property bool closable: true
    default property alias contentChildren: contentArea.data
    signal closeRequested()

    Rectangle {
        id: panelBg
        anchors.fill: parent
        radius: Theme.radiusLg
        color: Theme.surfaceElev
        border.color: Theme.border
        border.width: 1

        // MultiEffect needs a sibling source so the shadow renders
        // *under* the panel; layering on the rectangle itself works
        // for the simple case where the panel has no transparency.
        layer.enabled: true
        layer.effect: MultiEffect {
            shadowEnabled: true
            shadowBlur: 1.0
            shadowVerticalOffset: Theme.popupShadowOffset
            shadowHorizontalOffset: 0
            shadowColor: Theme.popupShadowColor
            shadowOpacity: 1.0
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacing
        spacing: Theme.spacing

        // Header — visible iff there's a title or a close pill.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing
            visible: root.title.length > 0 || root.closable

            Label {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.spacingXs
                text: root.title
                color: Theme.foreground
                font: Theme.headerFont
                elide: Text.ElideRight
            }
            IconButton {
                visible: root.closable
                iconKind: "x"
                onClicked: root.closeRequested()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.border
            visible: root.title.length > 0 || root.closable
        }

        Item {
            id: contentArea
            Layout.fillWidth: true
            Layout.fillHeight: true
        }
    }
}
