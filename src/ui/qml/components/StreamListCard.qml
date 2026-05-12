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
// Information hierarchy, primary to secondary:
//
//   1. Quality rail     — resolution chip. Top-tier signal: this is
//                         the first thing a user reads.
//   2. Summary column   — `source · codec · hdr · audio` + chips for
//                         languages / multi / release group /
//                         provider. Demoted to caption weight because
//                         the technical chrome is supporting context.
//   3. Metrics column   — seeders + size, right-aligned and bold.
//                         Top-tier signal: drives the play/skip choice.
//   4. Actions          — primary play/open button + overflow menu.
//                         The overflow menu doubles as the row's
//                         right-click context menu via the chassis.
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

    // Body slot (default): horizontal three-column layout. Wrapped
    // in a single RowLayout because the chassis' body slot stacks
    // children vertically.
    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.groupSpacing

        // ---- 1. Quality rail ---------------------------------
        ColumnLayout {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Math.round(
                Kirigami.Units.gridUnit * 5.5)
            spacing: Math.round(Theme.inlineSpacing / 2)

            MetaChip {
                Layout.alignment: Qt.AlignHCenter
                text: (!card.resolution || card.resolution === "\u2014")
                    ? i18nc("@label resolution unknown", "?")
                    : card.resolution.toUpperCase()
                tone: "neutral"
            }
        }

        // ---- 2. Summary column -------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Math.round(Theme.inlineSpacing / 2)

            QQC2.Label {
                id: summaryLabel
                Layout.fillWidth: true
                text: card.summaryLine.length > 0
                    ? card.summaryLine
                    : card.releaseName
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                font.weight: Font.Medium
                color: Theme.disabled
                onTruncatedChanged: card.titleTooltip
                    = (truncated && card.releaseName.length > 0)
                        ? card.releaseName
                        : ""
            }

            // Chip rail: tags + provider as a trailing caption.
            // RowChipRail strips empty entries, so the conditional
            // `provider` chip collapses without manual visibility
            // toggling.
            RowChipRail {
                Layout.fillWidth: true
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
                visible: chips.length > 0
                    || (card.provider && card.provider.length > 0)

                // Provider sits in the trailing slot as a small
                // caption rather than a chip so the rail stays
                // visually distinct from "release tag" metadata.
                QQC2.Label {
                    visible: card.provider
                        && card.provider.length > 0
                    text: card.provider
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }

        // ---- 3. Metrics column -------------------------------
        ColumnLayout {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.gridUnit * 6
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.alignment: Qt.AlignRight
                visible: card.seeders >= 0
                spacing: Math.round(Theme.inlineSpacing / 2)

                Kirigami.Icon {
                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                    Layout.preferredHeight: width
                    source: AppIcons.url("users")
                    color: Theme.foreground
                }
                QQC2.Label {
                    text: card.seeders
                    font.pointSize: Theme.defaultFont.pointSize
                    font.weight: Font.DemiBold
                    color: Theme.foreground
                    horizontalAlignment: Text.AlignRight
                }
            }

            QQC2.Label {
                Layout.alignment: Qt.AlignRight
                visible: card.sizeText && card.sizeText.length > 0
                text: card.sizeText
                font.pointSize: Theme.defaultFont.pointSize
                font.weight: Font.Medium
                color: Theme.foreground
                horizontalAlignment: Text.AlignRight
            }
        }
    }

    // Trailing slot: primary play / secondary download / overflow.
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

        QQC2.ToolButton {
            visible: card.hasDirectUrl || card.hasMagnet
            enabled: card.hasDirectUrl || card.hasMagnet
            icon.source: AppIcons.url("download",
                AppIcons.controlColor(enabled, false))
            icon.color: AppIcons.controlColor(enabled, false)
            display: QQC2.AbstractButton.IconOnly
            text: i18nc("@action:button background full download",
                "Download")
            QQC2.ToolTip.text: i18nc(
                "@info:tooltip background download",
                "Download the whole file in the background. "
                + "Continues even if you stop watching.")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: card.vm.download(card.row)
        }

        QQC2.ToolButton {
            icon.source: AppIcons.url("ellipsis")
            icon.color: AppIcons.controlColor(enabled, false)
            display: QQC2.AbstractButton.IconOnly
            text: i18nc("@action:button stream actions",
                "More actions")
            onClicked: {
                actionMenu.row = card.row;
                actionMenu.hasMagnet = card.hasMagnet;
                actionMenu.hasDirectUrl = card.hasDirectUrl;
                actionMenu.popup();
            }
        }
    }

    StreamRowActions {
        id: actionMenu
        vm: card.vm
    }
}
