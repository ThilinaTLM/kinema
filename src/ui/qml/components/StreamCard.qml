// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One stream row, tuned for scanability without making every field
// fight for top priority. Visually shaped to match `EpisodeRow` —
// alternate background colour, soft border, hover overlay — so the
// streams list reads as a stack of distinct cards rather than a
// flat block.
//
// Information hierarchy, primary to secondary:
//
//   1. Quality rail     — resolution chip + RD status badge. Top-tier
//                         signal: this is the first thing a user reads.
//   2. Metrics column   — seeders + size, right-aligned and bold.
//                         Top-tier signal: drives the play/skip choice.
//   3. Summary column   — `source · codec · hdr · audio`, plus chips
//                         for languages / multi / release group /
//                         provider. Demoted to caption weight because
//                         the technical chrome is supporting context,
//                         not the row's title.
//   4. Actions          — primary play/open button + overflow menu.
QQC2.ItemDelegate {
    id: card

    // ---- inputs from the model -------------------------------
    property int row: -1
    property string releaseName
    property string summaryLine
    property var tags: []
    property string sizeText
    property int seeders: -1
    property string provider
    property bool rdCached: false
    property bool rdDownload: false
    property bool hasMagnet: false
    property bool hasDirectUrl: false
    property string resolution

    /// View-model exposing the row action slots. Defaults to the
    /// movie detail VM; the series page rebinds it.
    property var vm: movieDetailVm

    width: ListView.view
        ? ListView.view.width - ListView.view.leftMargin - ListView.view.rightMargin
        : implicitWidth
    padding: Theme.pageMargin
    implicitHeight: layout.implicitHeight + padding * 2

    onDoubleClicked: card._activatePrimary()
    Keys.onReturnPressed: card._activatePrimary()
    Keys.onEnterPressed: card._activatePrimary()

    function _activatePrimary() {
        if (card.hasDirectUrl || card.hasMagnet) {
            card.vm.playNow(card.row);
        }
    }

    // Right-click anywhere on the row pops the action menu.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        propagateComposedEvents: true
        onClicked: function (mouse) {
            actionMenu.row = card.row;
            actionMenu.hasMagnet = card.hasMagnet;
            actionMenu.hasDirectUrl = card.hasDirectUrl;
            actionMenu.popup();
            mouse.accepted = true;
        }
    }

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.alternateBackgroundColor
        border.color: Qt.alpha(Theme.foreground,
            card.hovered ? 0.12 : 0.08)
        border.width: 1

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: card.hovered
                ? Qt.alpha(Theme.hover, 0.12)
                : "transparent"
        }
    }

    contentItem: RowLayout {
        id: layout
        spacing: Theme.groupSpacing

        // ---- 1. Quality rail -----------------------------------
        ColumnLayout {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Math.round(
                Kirigami.Units.gridUnit * 5.5)
            spacing: Math.round(Theme.inlineSpacing / 2)

            MetaChip {
                Layout.alignment: Qt.AlignHCenter
                text: (!card.resolution || card.resolution === "—")
                    ? i18nc("@label resolution unknown", "?")
                    : card.resolution.toUpperCase()
                tone: "neutral"
            }

            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                visible: card.rdCached || card.rdDownload
                radius: height / 2
                color: card.rdCached ? Theme.positive : Theme.accent
                implicitHeight: rdLabel.implicitHeight
                    + Theme.inlineSpacing
                implicitWidth: rdLabel.implicitWidth
                    + Theme.inlineSpacing * 2

                QQC2.Label {
                    id: rdLabel
                    anchors.centerIn: parent
                    text: card.rdCached
                        ? i18nc("@label stream chip", "RD+")
                        : i18nc("@label stream chip", "RD")
                    color: Theme.background
                    font.pointSize: Theme.captionFont.pointSize
                    font.weight: Font.DemiBold
                }
            }
        }

        // ---- 2. Summary column (secondary) ---------------------
        // Holds the technical line + chips. Demoted to caption /
        // disabled colour so resolution + seeders / size carry the
        // visual weight.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Math.round(Theme.inlineSpacing / 2)

            HoverHandler {
                id: hoverHandler
            }
            QQC2.ToolTip {
                visible: hoverHandler.hovered
                    && card.releaseName.length > 0
                delay: Kirigami.Units.toolTipDelay
                text: card.releaseName
            }

            QQC2.Label {
                Layout.fillWidth: true
                text: card.summaryLine.length > 0
                    ? card.summaryLine
                    : card.releaseName
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                font.weight: Font.Medium
                color: Theme.disabled
            }

            Flow {
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing
                visible: card.tags.length > 0
                    || (card.provider && card.provider.length > 0)

                Repeater {
                    model: card.tags
                    delegate: MetaChip {
                        required property string modelData
                        text: modelData
                        tone: "neutral"
                    }
                }

                QQC2.Label {
                    visible: card.provider && card.provider.length > 0
                    text: card.provider
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                    verticalAlignment: Text.AlignVCenter
                    height: Theme.defaultFont.pixelSize + Theme.inlineSpacing
                }
            }
        }

        // ---- 3. Metrics column (primary) -----------------------
        // Seeders carry top weight — a healthy swarm is the single
        // strongest signal a stream will play. Size sits beneath at
        // medium weight.
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

        // ---- 4. Primary action + overflow ----------------------
        RowLayout {
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.inlineSpacing

            QQC2.Button {
                visible: card.hasDirectUrl || card.hasMagnet
                enabled: card.hasDirectUrl || card.hasMagnet
                icon.source: AppIcons.url(card.hasDirectUrl
                    ? "play"
                    : "external-link",
                    AppIcons.controlColor(enabled, highlighted))
                icon.color: AppIcons.controlColor(enabled, highlighted)
                text: card.hasDirectUrl
                    ? i18nc("@action:button primary stream action",
                        "Play now")
                    : i18nc("@action:button primary stream action",
                        "Stream")
                display: QQC2.AbstractButton.TextBesideIcon
                highlighted: card.hasDirectUrl
                onClicked: card._activatePrimary()
            }

            QQC2.ToolButton {
                icon.source: AppIcons.url("ellipsis")
                icon.color: AppIcons.controlColor(enabled, false)
                display: QQC2.AbstractButton.IconOnly
                text: i18nc("@action:button stream actions", "More actions")
                onClicked: {
                    actionMenu.row = card.row;
                    actionMenu.hasMagnet = card.hasMagnet;
                    actionMenu.hasDirectUrl = card.hasDirectUrl;
                    actionMenu.popup();
                }
            }
        }
    }

    StreamRowActions {
        id: actionMenu
        vm: card.vm
    }
}
