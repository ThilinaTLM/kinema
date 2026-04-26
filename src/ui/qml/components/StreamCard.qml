// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One stream row: chip strip on the left, release name above details,
// size + seeders on the right, then a "\u22ee" actions button. Replaces
// the columnar `QTableView` row from the legacy `StreamsPanel` with a
// dense card layout that scales down to narrow viewports.
//
// Action routing: double-click / Enter triggers `play(row)`; the
// "\u22ee" button opens a `StreamRowActions` menu seeded with the same
// row index. Both go through the owning view-model.
QQC2.ItemDelegate {
    id: card

    // ---- inputs from the model -------------------------------
    property int row: -1
    property string releaseName
    property string detailsText
    property var chips: []
    property string sizeText
    property int seeders: -1
    property bool rdCached: false
    property bool rdDownload: false
    property bool hasMagnet: false
    property bool hasDirectUrl: false

    /// View-model exposing the row action slots. Defaults to the
    /// movie detail VM; commit B's SeriesDetailPage rebinds it.
    property var vm: movieDetailVm

    width: ListView.view ? ListView.view.width : implicitWidth
    padding: Kirigami.Units.largeSpacing
    implicitHeight: layout.implicitHeight + padding * 2

    onDoubleClicked: if (card.hasDirectUrl) card.vm.play(card.row)
    Keys.onReturnPressed: if (card.hasDirectUrl) card.vm.play(card.row)
    Keys.onEnterPressed:  if (card.hasDirectUrl) card.vm.play(card.row)

    // Right-click anywhere on the row pops the action menu \u2014 the
    // legacy QTableView did this, and users coming from the widget
    // expect it.
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

    contentItem: RowLayout {
        id: layout
        spacing: Kirigami.Units.largeSpacing

        // ---- left: chip strip + release name + details -------
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            // Chip row.
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: card.chips
                    delegate: MetaChip {
                        text: modelData
                        // Highlight the RD cached chip so it pops
                        // out of a wall of resolution / provider
                        // labels. The row uses the localised label
                        // ("RD+") so the comparison is robust to
                        // translation.
                        tone: (modelData === i18nc("@label stream chip", "RD+"))
                            ? "positive"
                            : ((modelData === i18nc("@label stream chip", "RD"))
                                ? "accent" : "neutral")
                    }
                }
                Item { Layout.fillWidth: true }
            }

            // Release name.
            QQC2.Label {
                Layout.fillWidth: true
                text: card.releaseName
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                font.pointSize: Theme.defaultFont.pointSize
                font.weight: Font.DemiBold
                color: Theme.foreground
            }

            // Details (codec / audio / source extras).
            QQC2.Label {
                Layout.fillWidth: true
                visible: card.detailsText.length > 0
                text: card.detailsText
                wrapMode: Text.WordWrap
                maximumLineCount: 1
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
            }
        }

        // ---- right: size + seeders --------------------------
        ColumnLayout {
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.gridUnit * 6
            spacing: 0

            QQC2.Label {
                Layout.alignment: Qt.AlignRight
                text: card.sizeText
                font.pointSize: Theme.defaultFont.pointSize
                font.weight: Font.DemiBold
                color: Theme.foreground
            }
            QQC2.Label {
                Layout.alignment: Qt.AlignRight
                text: card.seeders >= 0
                    ? i18nc("@info seeders", "\u21ea %1", card.seeders)
                    : i18nc("@info seeders unknown", "\u21ea \u2014")
                font.pointSize: Theme.captionFont.pointSize
                color: card.seeders > 5 ? Theme.positive : Theme.disabled
            }
        }

        // ---- "\u22ee" action button -----------------------------
        QQC2.ToolButton {
            Layout.alignment: Qt.AlignVCenter
            icon.name: "overflow-menu"
            display: QQC2.AbstractButton.IconOnly
            text: i18nc("@action:button stream actions", "Actions")
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
