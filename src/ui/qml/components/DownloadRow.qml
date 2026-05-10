// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One persistent download row in the Downloads page list. Designed
// to surface, in priority order:
//
//   1. Title + subtitle (release / quality / provider)
//   2. State pill + backend chip + (transient) live rate
//   3. Progress bar + cached/total + ETA
//   4. Peers/seeds (torrent only) and error pill (failed only)
//
// Action affordances on the right:
//   - Pin / Unpin (distinct icons)
//   - Retry (shown for Failed)
//   - Open folder
//   - Overflow menu: Remove from list / Remove and delete files
QQC2.ItemDelegate {
    id: row

    required property string assetId
    required property string title
    required property string subtitle
    required property string posterUrl
    required property int state
    required property string stateText
    required property string stateTone
    required property int backendKind
    required property string backendIcon
    required property string backendLabel
    required property bool pinned
    required property bool complete
    required property var progressFraction
    required property string progressText
    required property string sizeText
    required property string downloadRateText
    required property int peers
    required property int seeds
    required property string etaText
    required property string errorText
    required property string localDir
    required property string releaseName

    height: implicitContentHeight + topPadding + bottomPadding
    implicitHeight: Kirigami.Units.gridUnit * 7
    hoverEnabled: true

    background: Rectangle {
        color: row.hovered
            ? Kirigami.Theme.alternateBackgroundColor
            : "transparent"
        radius: Kirigami.Units.cornerRadius
    }

    function _toneColor(tone) {
        switch (tone) {
        case "positive":
            return Kirigami.Theme.positiveTextColor;
        case "negative":
            return Kirigami.Theme.negativeTextColor;
        case "warn":
            return Kirigami.Theme.neutralTextColor;
        default:
            return Kirigami.Theme.textColor;
        }
    }

    contentItem: RowLayout {
        spacing: Kirigami.Units.largeSpacing

        Image {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 3
            Layout.preferredHeight: Kirigami.Units.gridUnit * 4.5
            Layout.alignment: Qt.AlignTop
            fillMode: Image.PreserveAspectCrop
            source: row.posterUrl.length > 0
                ? "image://kinema/poster?u=" + encodeURIComponent(row.posterUrl)
                : ""
            Rectangle {
                anchors.fill: parent
                visible: !parent.source || parent.status !== Image.Ready
                color: Kirigami.Theme.alternateBackgroundColor
                radius: Kirigami.Units.cornerRadius
                Kirigami.Icon {
                    anchors.centerIn: parent
                    source: "media-optical"
                    width: Kirigami.Units.iconSizes.medium
                    height: width
                    color: Kirigami.Theme.disabledTextColor
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            spacing: Kirigami.Units.smallSpacing

            QQC2.Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: row.title
                font.bold: true
            }

            QQC2.Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                color: Kirigami.Theme.disabledTextColor
                text: row.subtitle
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                visible: row.subtitle.length > 0
            }

            // Chip rail: state pill, backend chip, rate, ETA
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing

                // State pill
                Rectangle {
                    radius: height / 2
                    color: Qt.alpha(row._toneColor(row.stateTone), 0.18)
                    border.color: Qt.alpha(row._toneColor(row.stateTone), 0.45)
                    border.width: 1
                    implicitHeight: stateLbl.implicitHeight
                        + Kirigami.Units.smallSpacing
                    implicitWidth: stateLbl.implicitWidth
                        + Kirigami.Units.largeSpacing * 1.5
                    QQC2.Label {
                        id: stateLbl
                        anchors.centerIn: parent
                        text: row.stateText
                        color: row._toneColor(row.stateTone)
                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                        font.weight: Font.DemiBold
                    }
                }

                // Backend chip (icon + label)
                RowLayout {
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon {
                        source: row.backendIcon
                        Layout.preferredWidth: Kirigami.Units.iconSizes.small
                        Layout.preferredHeight: width
                        color: Kirigami.Theme.disabledTextColor
                    }
                    QQC2.Label {
                        text: row.backendLabel
                        color: Kirigami.Theme.disabledTextColor
                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                    }
                }

                // Pinned badge
                QQC2.Label {
                    visible: row.pinned
                    text: i18nc("@label download pinned badge", "Pinned")
                    color: Kirigami.Theme.highlightColor
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    font.weight: Font.DemiBold
                }

                Item { Layout.fillWidth: true }

                QQC2.Label {
                    visible: row.downloadRateText.length > 0
                    text: row.downloadRateText
                    color: Kirigami.Theme.textColor
                    font.weight: Font.DemiBold
                }
                QQC2.Label {
                    visible: row.etaText.length > 0
                    text: i18nc("@label download eta", "ETA %1", row.etaText)
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            // Progress bar + size text
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing

                QQC2.ProgressBar {
                    Layout.fillWidth: true
                    visible: row.progressFraction !== undefined
                    from: 0
                    to: 1
                    value: row.progressFraction !== undefined
                        ? row.progressFraction
                        : 0
                }

                QQC2.Label {
                    text: row.sizeText
                    color: Kirigami.Theme.disabledTextColor
                }
                QQC2.Label {
                    visible: row.progressText.length > 0
                    text: row.progressText
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            // Torrent peers/seeds line (hidden for HTTP backend)
            QQC2.Label {
                Layout.fillWidth: true
                visible: row.backendKind === 0
                    && (row.peers > 0 || row.seeds > 0)
                color: Kirigami.Theme.disabledTextColor
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                text: i18ncp("@label torrent peers / seeds line, %1 peers, %2 seeds",
                    "%1 peer · %2 seeds", "%1 peers · %2 seeds",
                    row.peers, row.peers, row.seeds)
            }

            // Error pill (failed only)
            Rectangle {
                visible: row.errorText.length > 0
                Layout.fillWidth: true
                color: Qt.alpha(Kirigami.Theme.negativeTextColor, 0.12)
                border.color: Qt.alpha(Kirigami.Theme.negativeTextColor, 0.4)
                border.width: 1
                radius: Kirigami.Units.cornerRadius
                implicitHeight: errLbl.implicitHeight
                    + Kirigami.Units.smallSpacing * 2
                QQC2.Label {
                    id: errLbl
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.leftMargin: Kirigami.Units.smallSpacing
                    anchors.rightMargin: Kirigami.Units.smallSpacing
                    text: row.errorText
                    color: Kirigami.Theme.negativeTextColor
                    elide: Text.ElideRight
                    wrapMode: Text.NoWrap
                }
            }
        }

        // Action column
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                spacing: Kirigami.Units.smallSpacing
                Layout.alignment: Qt.AlignRight

                QQC2.ToolButton {
                    icon.name: row.pinned ? "pin" : "window-pin"
                    text: row.pinned
                        ? i18nc("@action:button", "Unpin")
                        : i18nc("@action:button", "Pin")
                    display: QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    onClicked: downloadsVm.pin(row.assetId, !row.pinned)
                }

                QQC2.ToolButton {
                    icon.name: "view-refresh"
                    text: i18nc("@action:button", "Retry")
                    display: QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    visible: row.state === 7 // Failed
                    onClicked: downloadsVm.retry(row.assetId)
                }

                QQC2.ToolButton {
                    icon.name: "folder-open"
                    text: i18nc("@action:button open the local cache folder",
                        "Open folder")
                    display: QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    onClicked: downloadsVm.openLocalDir(row.assetId)
                }

                QQC2.ToolButton {
                    icon.name: "overflow-menu"
                    text: i18nc("@action:button overflow", "More")
                    display: QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    onClicked: rowMenu.popup()

                    QQC2.Menu {
                        id: rowMenu
                        QQC2.MenuItem {
                            text: i18nc("@action:inmenu", "Cancel")
                            icon.name: "process-stop"
                            enabled: row.state !== 4 // Completed
                                && row.state !== 7 // Failed
                            onTriggered: downloadsVm.cancel(row.assetId)
                        }
                        QQC2.MenuSeparator { }
                        QQC2.MenuItem {
                            text: i18nc("@action:inmenu",
                                "Remove from list (keep files)")
                            icon.name: "list-remove"
                            onTriggered: downloadsVm.remove(row.assetId, false)
                        }
                        QQC2.MenuItem {
                            text: i18nc("@action:inmenu",
                                "Remove and delete files")
                            icon.name: "edit-delete"
                            onTriggered: deleteConfirm.open()
                        }
                    }

                    QQC2.Dialog {
                        id: deleteConfirm
                        modal: true
                        anchors.centerIn: QQC2.ApplicationWindow.overlay
                        title: i18nc("@title:dialog", "Delete cached files?")
                        standardButtons: QQC2.Dialog.Yes | QQC2.Dialog.No
                        contentItem: QQC2.Label {
                            text: i18nc("@info confirm before destructive remove",
                                "Remove this download and delete its cached files?")
                            wrapMode: Text.WordWrap
                        }
                        onAccepted: downloadsVm.remove(row.assetId, true)
                    }
                }
            }
        }
    }
}
