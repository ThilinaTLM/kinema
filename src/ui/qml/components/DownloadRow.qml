// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

QQC2.ItemDelegate {
    id: row

    required property string assetId
    required property string title
    required property string subtitle
    required property string posterUrl
    required property int state
    required property string stateText
    required property bool pinned
    required property bool complete
    required property var progressFraction
    required property string progressText
    required property string sizeText
    required property string errorText
    required property string localDir

    height: Kirigami.Units.gridUnit * 5
    hoverEnabled: true

    contentItem: RowLayout {
        spacing: Kirigami.Units.largeSpacing

        Image {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 3
            Layout.preferredHeight: Kirigami.Units.gridUnit * 4
            fillMode: Image.PreserveAspectCrop
            source: row.posterUrl.length > 0
                ? "image://kinema/poster?u=" + encodeURIComponent(row.posterUrl)
                : ""
        }

        ColumnLayout {
            Layout.fillWidth: true
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
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing

                QQC2.Label {
                    text: row.stateText
                    color: row.state === 7 // Failed
                        ? Kirigami.Theme.negativeTextColor
                        : Kirigami.Theme.textColor
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
                QQC2.ProgressBar {
                    Layout.fillWidth: true
                    visible: row.progressFraction !== undefined
                    from: 0
                    to: 1
                    value: row.progressFraction !== undefined ? row.progressFraction : 0
                }
            }
            QQC2.Label {
                visible: row.errorText.length > 0
                Layout.fillWidth: true
                elide: Text.ElideRight
                color: Kirigami.Theme.negativeTextColor
                text: row.errorText
            }
        }

        QQC2.ToolButton {
            icon.name: row.pinned ? "pin" : "pin"
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
            icon.name: "edit-delete"
            text: i18nc("@action:button", "Remove")
            display: QQC2.AbstractButton.IconOnly
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            onClicked: downloadsVm.remove(row.assetId, true)
        }
    }
}
