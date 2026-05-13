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

    // Line 2: unified meta row. Four zones, separated by
    // `largeSpacing` whitespace only — no inter-zone middots, so
    // the eye reads the line as chunks rather than a flat dot-
    // separated chain. Within the tech summary the middots are
    // already baked into `summaryLine`, which is the only zone
    // that carries a within-zone rhythm.
    RowLayout {
        readonly property bool hasTags:
            card.tags && card.tags.length > 0
        readonly property bool hasSummary:
            card.summaryLine.length > 0
        readonly property bool hasProvider:
            card.provider && card.provider.length > 0
        readonly property bool hasSeeders: card.seeders >= 0

        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

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

        // Zone (b): tech summary, caption/disabled. Elides right
        // when the row narrows. `Layout.alignment` is set
        // explicitly because QtQuick.Layouts defaults children to
        // top-aligned, which would drop this label off the row's
        // vertical center against the chip rail and seeders icon.
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

        // Zone (d): seeders. Icon as the implicit qualifier, plain
        // number alongside (no "seeders" suffix, regular weight —
        // matches the rest of the caption-toned line). Wrapped in a
        // RowLayout so the icon and number stay tight; the outer
        // largeSpacing only separates the cluster from zone (c).
        // The inner icon and label both carry an explicit
        // `Layout.alignment: Qt.AlignVCenter` so they share a
        // vertical center inside the nested layout — without it,
        // QtQuick.Layouts would top-align them and the label would
        // drop below the sibling labels' baseline.
        RowLayout {
            visible: parent.hasSeeders
            spacing: Theme.inlineSpacing
            Layout.alignment: Qt.AlignVCenter

            Kirigami.Icon {
                Layout.alignment: Qt.AlignVCenter
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: width
                source: AppIcons.url("users")
                color: Theme.disabled
            }
            QQC2.Label {
                Layout.alignment: Qt.AlignVCenter
                text: card.seeders
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
                verticalAlignment: Text.AlignVCenter
                QQC2.ToolTip.text: i18ncp(
                    "@info:tooltip swarm seeders",
                    "%1 seeder", "%1 seeders", card.seeders)
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                HoverHandler { id: seedersHover }
                property bool hovered: seedersHover.hovered
            }
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
