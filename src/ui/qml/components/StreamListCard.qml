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
//   2. Meta row         — four visually distinct zones on a single
//                         left-flowing line, separated by `large-
//                         Spacing` whitespace only:
//                           a. chips      (languages / multi / group)
//                           b. tech       (`source · codec · hdr ·
//                                          audio` — middots stay
//                                          *within* this zone)
//                           c. provider   (plain caption label)
//                           d. seeders    (users icon + number)
//                         Middots are reserved for within-zone
//                         rhythm; between zones we use spacing
//                         alone, so unlike kinds of metadata aren't
//                         glued into a single dot-separated chain.
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

    // Line 2: unified meta row. Four zones — chips, tech summary,
    // provider, seeders — separated by inter-zone whitespace
    // only, no middots, so unlike kinds of metadata aren't glued
    // into a single dot-separated chain.
    //
    // Implementation note: every visible item is a *direct* child
    // of this `RowLayout` and carries `Layout.alignment:
    // Qt.AlignVCenter` so they share a single vertical-center axis.
    // The earlier version wrapped the seeders icon + count in a
    // nested `RowLayout` for tight icon-number spacing; that
    // introduced a two-level center hierarchy whose inner items
    // ended up rendering off the sibling labels' baseline. Instead,
    // the row uses tight `inlineSpacing` throughout and explicit
    // `zoneGap`-wide `Item` spacers separate the zones, so the
    // icon and count sit next to each other naturally without a
    // nested layout.
    RowLayout {
        readonly property bool hasTags:
            card.tags && card.tags.length > 0
        readonly property bool hasSummary:
            card.summaryLine.length > 0
        readonly property bool hasProvider:
            card.provider && card.provider.length > 0
        readonly property bool hasSeeders: card.seeders >= 0
        readonly property int zoneGap: Kirigami.Units.largeSpacing

        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        // Zone (a): tag chips.
        RowChipRail {
            Layout.alignment: Qt.AlignVCenter
            visible: parent.hasTags
            chips: {
                const list = [];
                if (card.tags) {
                    for (let i = 0; i < card.tags.length; ++i) {
                        list.push({
                            text: card.tags[i],
                            tone: "neutral"
                        });
                    }
                }
                return list;
            }
        }

        // Zone separator (a) → (b): only visible when both adjacent
        // zones are present.
        Item {
            visible: parent.hasTags && parent.hasSummary
            Layout.preferredWidth: parent.zoneGap
                - parent.spacing * 2
            Layout.preferredHeight: 1
        }

        // Zone (b): tech summary, caption/disabled. Elides right
        // when the row narrows.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: parent.hasSummary
            text: card.summaryLine
            wrapMode: Text.NoWrap
            elide: Text.ElideRight
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Zone separator (b) → (c).
        Item {
            visible: (parent.hasTags || parent.hasSummary)
                && parent.hasProvider
            Layout.preferredWidth: parent.zoneGap
                - parent.spacing * 2
            Layout.preferredHeight: 1
        }

        // Zone (c): provider name, caption/disabled. Plain text
        // — it's chrome, not a metric.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: parent.hasProvider
            text: card.provider
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Zone separator (c) → (d).
        Item {
            visible: (parent.hasTags || parent.hasSummary
                || parent.hasProvider) && parent.hasSeeders
            Layout.preferredWidth: parent.zoneGap
                - parent.spacing * 2
            Layout.preferredHeight: 1
        }

        // Zone (d), part 1: users icon. Same vertical-center axis
        // as every other item on the line. Sized down from the
        // standard `iconSizes.small` so the icon reads as inline
        // chrome next to the caption-sized count rather than as
        // its own visual feature.
        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            visible: parent.hasSeeders
            Layout.preferredWidth:
                Math.round(Kirigami.Units.iconSizes.small * 0.8)
            Layout.preferredHeight: width
            source: AppIcons.url("users")
            color: Theme.disabled
        }

        // Zone (d), part 2: seeders count.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: parent.hasSeeders
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
