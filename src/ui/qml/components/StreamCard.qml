// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One stream row, redesigned around the question users actually
// answer here: "is this cached on Real-Debrid, what quality, what
// codec/source/audio, and how do I play it?"
//
// Layout (left to right):
//   1. Quality block        \u2014 resolution as a bold label, with a
//                              prominent RD+ / RD badge stacked
//                              under it. Cached rows get a thin
//                              accent stripe at the leading edge so
//                              they pop out of a wall of similar
//                              uncached releases.
//   2. Summary column       \u2014 line 1: pre-joined human summary
//                              (`source \u00b7 codec \u00b7 hdr \u00b7 audio`).
//                              line 2: a row of small chips (codec,
//                              HDR, language, multi-audio, group).
//                              line 3: technical subtitle
//                              (`size \u00b7 \u21ea seeders \u00b7 provider`).
//                              The full release name lives in a
//                              tooltip so it's available on demand
//                              without eating two lines of vertical
//                              space.
//   3. Action column        \u2014 a primary "Play" / "Open magnet"
//                              button + the existing `\u22ee` overflow
//                              that opens `StreamRowActions`.
//
// Right-clicking anywhere on the row still opens the overflow menu;
// double-clicking and Enter/Return play (or open magnet) the row.
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

    width: ListView.view ? ListView.view.width : implicitWidth
    padding: Kirigami.Units.largeSpacing
    implicitHeight: layout.implicitHeight + padding * 2

    onDoubleClicked: card._activatePrimary()
    Keys.onReturnPressed: card._activatePrimary()
    Keys.onEnterPressed: card._activatePrimary()

    function _activatePrimary() {
        if (card.hasDirectUrl) {
            card.vm.play(card.row);
        } else if (card.hasMagnet) {
            card.vm.openMagnet(card.row);
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

    // Subtle leading accent stripe for cached rows. Drawn on top of
    // the delegate's background so it's visible against highlight.
    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 3
        color: card.rdCached ? Theme.positive : "transparent"
        z: 1
    }

    contentItem: RowLayout {
        id: layout
        spacing: Kirigami.Units.largeSpacing

        // ---- 1. Quality block ----------------------------------
        ColumnLayout {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.gridUnit * 5
            spacing: Math.round(Kirigami.Units.smallSpacing / 2)

            QQC2.Label {
                Layout.alignment: Qt.AlignHCenter
                text: (!card.resolution || card.resolution === "\u2014")
                    ? i18nc("@label resolution unknown", "?")
                    : card.resolution.toUpperCase()
                font.pointSize: Theme.defaultFont.pointSize + 1
                font.weight: Font.Bold
                color: Theme.foreground
            }

            // RD+ / RD badge \u2014 a filled pill, the *one* loud thing
            // on the row. Hidden entirely for non-RD rows.
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                visible: card.rdCached || card.rdDownload
                radius: height / 2
                color: card.rdCached ? Theme.positive : Theme.accent
                implicitHeight: rdLabel.implicitHeight
                    + Kirigami.Units.smallSpacing
                implicitWidth: rdLabel.implicitWidth
                    + Kirigami.Units.smallSpacing * 3
                QQC2.Label {
                    id: rdLabel
                    anchors.centerIn: parent
                    text: card.rdCached
                        ? i18nc("@label stream chip", "RD+")
                        : i18nc("@label stream chip", "RD")
                    color: Theme.background
                    font.pointSize: Theme.captionFont.pointSize
                    font.weight: Font.Bold
                }
            }
        }

        // ---- 2. Summary + tags + technical subtitle -----------
        ColumnLayout {
            id: middleColumn
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Math.round(Kirigami.Units.smallSpacing / 2)

            // Tooltip exposes the raw release name on hover or focus.
            HoverHandler {
                id: hoverHandler
            }
            QQC2.ToolTip {
                visible: hoverHandler.hovered
                    && card.releaseName.length > 0
                delay: Kirigami.Units.toolTipDelay
                text: card.releaseName
            }

            // Line 1: human summary line. Falls back to the release
            // name when the parser couldn't extract anything useful.
            QQC2.Label {
                Layout.fillWidth: true
                text: card.summaryLine.length > 0
                    ? card.summaryLine
                    : card.releaseName
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
                font.pointSize: Theme.defaultFont.pointSize
                font.weight: Font.DemiBold
                color: Theme.foreground
            }

            // Line 2: small chips for codec / HDR / lang / group.
            // Only rendered when the parser produced any.
            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                visible: card.tags.length > 0
                Repeater {
                    model: card.tags
                    delegate: MetaChip {
                        required property string modelData
                        text: modelData
                        // Highlight the HDR / DV chip: that's a hard
                        // requirement for some users (TV capability).
                        tone: (modelData === i18nc("@label HDR profile", "Dolby Vision")
                                || modelData === i18nc("@label HDR profile", "HDR10+"))
                            ? "accent"
                            : (modelData === i18nc("@label HDR profile", "HDR10")
                                ? "accent" : "neutral")
                    }
                }
            }

            // Line 3: technical subtitle. Skips empty fields cleanly.
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
                text: {
                    var parts = [];
                    if (card.sizeText && card.sizeText.length > 0) {
                        parts.push(card.sizeText);
                    }
                    if (card.seeders >= 0) {
                        parts.push(i18nc("@info seeders",
                            "\u21ea %1", card.seeders));
                    }
                    if (card.provider && card.provider.length > 0) {
                        parts.push(card.provider);
                    }
                    return parts.join(" \u00b7 ");
                }
            }
        }

        // ---- 3. Primary action + overflow ---------------------
        RowLayout {
            Layout.alignment: Qt.AlignVCenter
            spacing: Kirigami.Units.smallSpacing

            QQC2.Button {
                id: primaryButton
                visible: card.hasDirectUrl || card.hasMagnet
                enabled: card.hasDirectUrl || card.hasMagnet
                icon.name: card.hasDirectUrl
                    ? "media-playback-start"
                    : "document-open"
                text: card.hasDirectUrl
                    ? i18nc("@action:button primary stream action", "Play")
                    : i18nc("@action:button primary stream action",
                        "Open magnet")
                display: QQC2.AbstractButton.TextBesideIcon
                highlighted: card.hasDirectUrl
                onClicked: card._activatePrimary()
            }

            QQC2.ToolButton {
                icon.name: "overflow-menu"
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
