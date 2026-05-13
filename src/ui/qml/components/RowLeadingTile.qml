// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Synthetic leading tile for list rows that don't carry artwork.
// Used by `StreamListCard` (resolution) and `SubtitleListCard`
// (language code) so every list row keeps the same 2-column
// silhouette as `EpisodeListCard` and `DownloadListCard`.
//
// Sized by the caller: set `Layout.preferredHeight` (typically
// `Kirigami.Units.gridUnit * 4` to match the canonical leading
// height) and the tile derives width from `aspect`. Default is
// a square (aspect 1.0).
Item {
    id: tile

    property string primary
    property string caption
    property var iconSource
    property color tint: Qt.alpha(Theme.foreground, 0.06)
    property color borderColor: Qt.alpha(Theme.foreground, 0.10)
    property real aspect: 1.0

    /// When true a hairline divider is drawn between `primary` and
    /// `caption`, turning the tile into a stacked two-field badge
    /// (e.g. resolution / size on StreamListCard). Defaults to off
    /// so subtitle / other consumers keep their unified look.
    property bool divided: false

    implicitWidth: Math.round(implicitHeight * aspect)
    implicitHeight: Kirigami.Units.gridUnit * 4
    Layout.preferredWidth: Math.round(Layout.preferredHeight * aspect)

    Rectangle {
        anchors.fill: parent
        radius: Kirigami.Units.cornerRadius
        color: tile.tint
        border.color: tile.borderColor
        border.width: 1

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            spacing: Math.round(Kirigami.Units.smallSpacing / 2)

            Item { Layout.fillHeight: true }

            Kirigami.Icon {
                visible: tile.iconSource !== undefined
                    && ("" + tile.iconSource).length > 0
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: width
                source: tile.iconSource
                color: Theme.foreground
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: tile.primary.length > 0
                text: tile.primary
                horizontalAlignment: Text.AlignHCenter
                color: Theme.foreground
                font.pointSize: Theme.defaultFont.pointSize + 1
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }

            // Hairline divider between primary + caption. Opt-in so
            // the unified tile (no divider) stays the default. Inset
            // slightly from the tile edge so it reads as a rule, not
            // a second border.
            Rectangle {
                visible: tile.divided
                    && tile.primary.length > 0
                    && tile.caption.length > 0
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.smallSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                Layout.topMargin: Math.round(
                    Kirigami.Units.smallSpacing / 2)
                Layout.bottomMargin: Math.round(
                    Kirigami.Units.smallSpacing / 2)
                Layout.preferredHeight: 1
                color: tile.borderColor
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: tile.caption.length > 0
                text: tile.caption
                horizontalAlignment: Text.AlignHCenter
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
                elide: Text.ElideRight
            }

            Item { Layout.fillHeight: true }
        }
    }
}
