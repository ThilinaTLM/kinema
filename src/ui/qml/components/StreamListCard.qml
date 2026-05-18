// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One stream row. Built on `BaseListCard`, which owns the row chrome,
// hover / focus styling, padding, width, right-click signalling.
//
// Layout, top to bottom in the right column, all left-aligned:
//
//   1. Release line     — full `releaseName`, foreground / Medium.
//                         The strongest identifier, so it leads.
//   2. Meta row         — single dot-separated caption line:
//                         `<tag> · <tag> · … · source · codec ·
//                          hdr · audio · provider · ⌂ seeders`.
//                         Every token renders as caption-sized
//                         text (with the seeders icon sized to the
//                         same baseline), so the line height is
//                         identical whether or not optional tokens
//                         are present. This replaces an earlier
//                         four-zone layout that mixed bordered
//                         `MetaChip` pills (tags) with caption
//                         text (tech / provider / seeders); the
//                         chip's vertical padding made the line
//                         taller whenever any tag was present and
//                         the row jittered between heights as the
//                         user scrolled.
//   3. Action row       — `Play`, `Download`, `More` as regular
//                         text-beside-icon buttons. No tool or
//                         icon-only buttons anywhere on the row.
//
// The leading tile is a stacked two-field badge: resolution on top,
// size below, separated by a hairline divider. Size therefore moves
// out of the meta row entirely.
BaseListCard {
    id: card

    // ---- inputs from the model -------------------------------
    property int row: -1
    property string releaseName
    property string summaryLine
    property var tags: []
    property string sizeText
    property int seeders: -1
    property string provider
    property bool hasMagnet: false
    property bool hasDirectUrl: false
    property string resolution
    /// Persistence token from `domain::providerToString`:
    /// "none" / "realdebrid" / "alldebrid". Anything else is
    /// treated as "none" so unknown providers don't show a badge.
    property string debridProvider: "none"
    /// True when the upstream indexer signalled the row is already
    /// cached on the debrid account. Drives the badge tone
    /// (positive vs neutral). Only meaningful when
    /// `debridProvider != "none"`.
    property bool debridCached: false

    /// View-model exposing the row action slots. Defaults to the
    /// movie detail VM; the series page rebinds it.
    property var vm: movieDetailVm

    readonly property string _emDash: "\u2014"

    function _activatePrimary() {
        if (card.hasDirectUrl || card.hasMagnet) {
            card.vm.playNow(card.row);
        }
    }

    onActivated: card._activatePrimary()

    onContextMenuRequested: function (pt) {
        actionMenu.row = card.row;
        actionMenu.hasMagnet = card.hasMagnet;
        actionMenu.hasDirectUrl = card.hasDirectUrl;
        actionMenu.popup(pt.x, pt.y);
    }

    // Leading tile: stacked resolution / size badge. The tile is
    // wider than tall (aspect 1.3) so the size string fits at
    // caption weight without crowding the resolution. Height still
    // tracks the right column via `fillHeight`, matching every
    // other list row.
    leading: RowLeadingTile {
        Layout.fillHeight: true
        Layout.preferredWidth: Math.round(height * 1.3)
        divided: true
        primary: (!card.resolution || card.resolution === card._emDash)
            ? i18nc("@label resolution unknown", "?")
            : card.resolution.toUpperCase()
        caption: (card.sizeText && card.sizeText.length > 0)
            ? card.sizeText
            : card._emDash

        // Debrid corner badge: "RD" / "AD" overlaid on the tile,
        // tinted positive when cached and neutral otherwise.
        // Rows without a debrid signal leave `badgeText` empty
        // and the tile renders exactly as before.
        badgeText: card.debridProvider === "realdebrid" ? "RD"
            : card.debridProvider === "alldebrid" ? "AD"
            : ""
        badgeTone: card.debridCached ? "positive" : "neutral"
        badgeTooltip: card.debridProvider === "realdebrid"
            ? (card.debridCached
                ? i18nc("@info:tooltip debrid cache state",
                    "Real-Debrid · cached (instant playback)")
                : i18nc("@info:tooltip debrid cache state",
                    "Real-Debrid · not cached (will be queued in your account)"))
            : card.debridProvider === "alldebrid"
                ? (card.debridCached
                    ? i18nc("@info:tooltip debrid cache state",
                        "AllDebrid · cached (instant playback)")
                    : i18nc("@info:tooltip debrid cache state",
                        "AllDebrid · not cached (will be queued in your account)"))
                : ""
    }

    // Line 1: release name. Foreground / Medium so it reads as the
    // row's primary identifier. Surfaces as a tooltip when elided.
    QQC2.Label {
        id: releaseLabel
        Layout.fillWidth: true
        visible: card.releaseName.length > 0
        text: card.releaseName
        wrapMode: Text.NoWrap
        elide: Text.ElideRight
        font.weight: Font.Medium
        color: Theme.foreground
        onTruncatedChanged: card.titleTooltip
            = (truncated && card.releaseName.length > 0)
                ? card.releaseName
                : ""
    }

    // Line 2: dot-separated caption tokens — tags, tech summary,
    // provider, seeders icon + count. Every visible token is a
    // *direct* child of this `RowLayout` carrying
    // `Layout.alignment: Qt.AlignVCenter` so they share a single
    // vertical-center axis and a single caption-sized line height
    // regardless of which tokens render.
    RowLayout {
        id: metaRow

        readonly property bool hasTags:
            card.tags && card.tags.length > 0
        readonly property bool hasSummary:
            card.summaryLine.length > 0
        readonly property bool hasProvider:
            card.provider && card.provider.length > 0
        readonly property bool hasSeeders: card.seeders >= 0

        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        // Tag tokens (languages / multi / group). Iterated as
        // plain caption labels; the row no longer uses pill chips
        // here, so the line height stays caption-sized in every
        // state.
        Repeater {
            model: metaRow.hasTags ? card.tags : []

            delegate: RowLayout {
                required property int index
                required property string modelData
                Layout.alignment: Qt.AlignVCenter
                spacing: Theme.inlineSpacing

                // Inter-tag `·` separator; first tag has none.
                QQC2.Label {
                    Layout.alignment: Qt.AlignVCenter
                    visible: index > 0
                    text: "\u00b7"
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                    verticalAlignment: Text.AlignVCenter
                }

                QQC2.Label {
                    Layout.alignment: Qt.AlignVCenter
                    text: modelData
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }

        // Separator (tags) → (tech summary).
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasTags && metaRow.hasSummary
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Tech summary (source · codec · hdr · audio — middots
        // baked into the string upstream). Elides right when the
        // row narrows.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasSummary
            text: card.summaryLine
            wrapMode: Text.NoWrap
            elide: Text.ElideRight
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Separator (tags|tech) → (provider).
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasTags || metaRow.hasSummary)
                && metaRow.hasProvider
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Provider name, caption/disabled. Plain text — it's
        // chrome, not a metric.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasProvider
            text: card.provider
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Separator (tags|tech|provider) → (seeders).
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasTags || metaRow.hasSummary
                || metaRow.hasProvider) && metaRow.hasSeeders
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Seeders icon. Sized down from the standard
        // `iconSizes.small` so it reads as inline chrome next to
        // the caption-sized count rather than as its own visual
        // feature.
        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasSeeders
            Layout.preferredWidth:
                Math.round(Kirigami.Units.iconSizes.small * 0.8)
            Layout.preferredHeight: width
            source: AppIcons.url("users")
            color: Theme.disabled
        }

        // Seeders count.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasSeeders
            text: card.seeders
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
            QQC2.ToolTip.text: i18ncp(
                "@info:tooltip swarm seeders",
                "%1 seeder", "%1 seeders", card.seeders)
            QQC2.ToolTip.visible: seedersHover.hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            HoverHandler { id: seedersHover }
        }

        // Trailing fill absorbs slack so the meta row stays packed
        // flush left when the body stretches it to the card width.
        Item { Layout.fillWidth: true }
    }

    // Action row: all regular text-beside-icon buttons, left-
    // aligned. No tool or icon-only buttons. The chassis renders
    // `trailing` below the body, so this reads as a dedicated
    // action band, not a right-edge cluster.
    trailing: RowLayout {
        spacing: Theme.inlineSpacing

        QQC2.Button {
            visible: card.hasDirectUrl || card.hasMagnet
            enabled: card.hasDirectUrl || card.hasMagnet
            icon.source: AppIcons.url("play",
                AppIcons.controlColor(enabled, highlighted))
            icon.color: AppIcons.controlColor(enabled, highlighted)
            text: i18nc("@action:button primary stream action",
                "Play")
            display: QQC2.AbstractButton.TextBesideIcon
            highlighted: card.hasDirectUrl
            onClicked: card._activatePrimary()
        }

        QQC2.Button {
            visible: card.hasDirectUrl || card.hasMagnet
            enabled: card.hasDirectUrl || card.hasMagnet
            icon.source: AppIcons.url("download",
                AppIcons.controlColor(enabled, false))
            icon.color: AppIcons.controlColor(enabled, false)
            text: i18nc("@action:button background full download",
                "Download")
            display: QQC2.AbstractButton.TextBesideIcon
            QQC2.ToolTip.text: i18nc(
                "@info:tooltip background download",
                "Download the whole file in the background. "
                + "Continues even if you stop watching.")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: card.vm.download(card.row)
        }

        QQC2.Button {
            icon.source: AppIcons.url("ellipsis",
                AppIcons.controlColor(true, false))
            icon.color: AppIcons.controlColor(true, false)
            text: i18nc("@action:button stream actions", "More")
            display: QQC2.AbstractButton.TextBesideIcon
            onClicked: {
                actionMenu.row = card.row;
                actionMenu.hasMagnet = card.hasMagnet;
                actionMenu.hasDirectUrl = card.hasDirectUrl;
                actionMenu.popup();
            }
        }

        // Trailing fill keeps the action row left-packed against
        // the start of the right column.
        Item { Layout.fillWidth: true }
    }

    StreamRowActions {
        id: actionMenu
        vm: card.vm
    }
}
