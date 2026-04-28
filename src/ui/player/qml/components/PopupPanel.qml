// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
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

    // Pin the panel subtree to the titlebar palette so any stock
    // QQC2 control inside (Label, TextField, ComboBox, Button …)
    // resolves through the same dark register as our bespoke chrome.
    // Combined with `QQuickStyle::setStyle("org.kde.desktop")` in
    // main.cpp this keeps SubtitleSearchSheet's text inputs dark and
    // Breeze-styled instead of falling back to the default light QQC2
    // look. `inherit: false` blocks any outer override from leaking
    // back in.
    Kirigami.Theme.colorSet: Kirigami.Theme.Header
    Kirigami.Theme.inherit: false

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

    // Implicit size from inner content + symmetric margins so callers
    // can size the panel to fit ("width: panel.implicitWidth") rather
    // than hardcoding pixels. Only used by overlays that opt into
    // content-driven sizing; fixed-size callers ignore these.
    implicitWidth:  layoutColumn.implicitWidth + 2 * Theme.spacing
    implicitHeight: layoutColumn.implicitHeight + 2 * Theme.spacing

    ColumnLayout {
        id: layoutColumn
        anchors.fill: parent
        anchors.margins: Theme.spacing
        spacing: Theme.spacing

        // Header — visible iff there's a title or a close pill.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing
            visible: root.title.length > 0 || root.closable

            // Heading typography resolves through the inherited Header
            // colorSet (pinned on the panel root above), so the title
            // picks Plasma's heading rhythm without losing the
            // dark-over-video surface.
            Kirigami.Heading {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.spacingXs
                text: root.title
                level: 2
                elide: Text.ElideRight
            }
            IconButton {
                visible: root.closable
                iconKind: "x"
                onClicked: root.closeRequested()
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            visible: root.title.length > 0 || root.closable
        }

        Item {
            id: contentArea
            Layout.fillWidth: true
            Layout.fillHeight: true
        }
    }
}
