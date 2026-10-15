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

    /// Optional corner-badge slot, rendered as a small pill in the
    /// top-right corner of the tile. Empty hides the badge
    /// entirely — non-stream consumers (e.g. SubtitleListCard) get
    /// the same look as before. Kept short (two characters,
    /// caption-sized) so it never competes with `primary`.
    property string badgeText: ""
    /// Theme role for the pill background and text. "positive" maps
    /// to `Kirigami.Theme.positive*`, anything else (default) to
    /// `Kirigami.Theme.neutral*`. Colours come exclusively from the
    /// Kirigami palette per AGENTS.md.
    property string badgeTone: "neutral"
    /// Tooltip surfaced on hover. Empty disables the tooltip.
    property string badgeTooltip: ""

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

        // Corner badge: small pill anchored to the top-right of the
        // tile chrome. Sits *inside* the tile border so the tile's
        // sizing / aspect / inner layout are untouched when the
        // badge appears. Visible only when `badgeText` is set.
        Rectangle {
            id: badge
            readonly property bool positive: tile.badgeTone === "positive"
            readonly property color bgColor: positive
                ? Kirigami.Theme.positiveBackgroundColor
                : Kirigami.Theme.neutralBackgroundColor
            readonly property color fgColor: positive
                ? Kirigami.Theme.positiveTextColor
                : Kirigami.Theme.neutralTextColor

            visible: tile.badgeText.length > 0
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Math.round(
                Kirigami.Units.smallSpacing / 2)
            implicitWidth: badgeLabel.implicitWidth
                + Kirigami.Units.smallSpacing * 2
            implicitHeight: badgeLabel.implicitHeight
                + Math.round(Kirigami.Units.smallSpacing * 0.6)
            radius: height / 2
            color: bgColor
            border.color: Qt.alpha(fgColor, 0.45)
            border.width: 1

            QQC2.Label {
                id: badgeLabel
                anchors.centerIn: parent
                text: tile.badgeText
                font.pointSize: Theme.captionFont.pointSize
                font.weight: Font.DemiBold
                color: badge.fgColor
            }

            HoverHandler { id: badgeHover }
            QQC2.ToolTip.text: tile.badgeTooltip
            QQC2.ToolTip.visible: badgeHover.hovered
                && tile.badgeTooltip.length > 0
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }
}
