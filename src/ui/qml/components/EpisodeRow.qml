// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One episode row: thumbnail · "1 \u00b7 Title" · release date /
// upcoming pill, with a one-line description below. Tapping the row
// pushes `StreamsPage` for that episode — the trailing `go-next`
// glyph signals that forward navigation.
QQC2.ItemDelegate {
    id: row

    property int episodeNumber: 0
    property string episodeTitle
    property string description
    property string releasedText
    property bool isUpcoming: false
    property string thumbnailUrl
    property bool selected: false
    property bool watched: false
    property real progress: -1

    signal toggleWatchedRequested()

    width: ListView.view ? ListView.view.width : implicitWidth
    padding: Theme.pageMargin
    implicitHeight: layout.implicitHeight + padding * 2

    background: Rectangle {
        color: row.selected
            ? Qt.alpha(Theme.accent, 0.18)
            : (row.hovered
                ? Kirigami.Theme.alternateBackgroundColor
                : "transparent")
        radius: Kirigami.Units.cornerRadius
    }

    contentItem: RowLayout {
        id: layout
        spacing: Theme.groupSpacing

        // Thumbnail. 16:9 to match Cinemeta's `still` shape; sized off
        // gridUnit so it scales with the user's KDE font / DPI.
        Item {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 7
            Layout.preferredHeight: Math.round(width * 9 / 16)

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.10)
                border.width: 1
            }
            Image {
                id: thumb
                anchors.fill: parent
                anchors.margins: 1
                source: row.thumbnailUrl
                    ? "image://kinema/still?u=" + encodeURIComponent(row.thumbnailUrl)
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                visible: status === Image.Ready
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.medium
                height: width
                source: AppIcons.url("play")
                color: Qt.alpha(Theme.foreground, 0.35)
                visible: thumb.status !== Image.Ready
            }
        }

        // Meta column.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.inlineSpacing

            RowLayout {
                spacing: Theme.inlineSpacing
                QQC2.Label {
                    text: i18nc("@info episode title prefix",
                        "%1 \u00b7 %2",
                        row.episodeNumber, row.episodeTitle)
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                    font.pointSize: Theme.defaultFont.pointSize
                    font.weight: Font.DemiBold
                    color: Theme.foreground
                }
                Kirigami.Icon {
                    visible: row.watched
                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                    Layout.preferredHeight: width
                    source: AppIcons.url("circle-check")
                    color: Theme.positive
                }
                UpcomingBadge {
                    visible: row.isUpcoming
                    releaseDateText: row.releasedText
                }
                QQC2.Label {
                    visible: !row.isUpcoming
                        && row.releasedText.length > 0
                    text: row.releasedText
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: row.description.length > 0
                text: row.description
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.smallSpacing
                visible: row.progress > 0 && row.progress < 1 && !row.watched
                radius: height / 2
                color: Qt.alpha(Theme.foreground, 0.14)
                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: parent.width * Math.max(0, Math.min(1, row.progress))
                    radius: parent.radius
                    color: Theme.accent
                }
            }
        }

        QQC2.ToolButton {
            Layout.alignment: Qt.AlignVCenter
            icon.source: AppIcons.url(row.watched ? "circle-dashed" : "circle-check")
            icon.color: row.watched ? Theme.positive : Theme.disabled
            text: row.watched
                ? i18nc("@action:button", "Mark unwatched")
                : i18nc("@action:button", "Mark watched")
            display: QQC2.AbstractButton.IconOnly
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: row.toggleWatchedRequested()
        }

        // Open-affordance chevron. Tapping a row pushes `StreamsPage`,
        // so a forward-navigation arrow is more honest than an
        // expand/collapse glyph (the row never expands inline).
        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            width: Kirigami.Units.iconSizes.small
            height: width
            source: AppIcons.url("chevron-right")
            color: row.selected ? Theme.accent : Theme.disabled
        }
    }
}
