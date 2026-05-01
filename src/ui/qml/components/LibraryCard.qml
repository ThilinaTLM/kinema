// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

QQC2.ItemDelegate {
    id: card

    property string posterUrl
    property string title
    property string subtitle
    property real progress: -1
    property bool watched: false
    property bool upcoming: false
    property string releaseDateText

    signal resumeRequested()

    padding: 0
    implicitWidth: Theme.posterMin
    implicitHeight: posterBox.implicitHeight + meta.implicitHeight
        + Kirigami.Units.smallSpacing

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: card.hovered ? Kirigami.Theme.alternateBackgroundColor : "transparent"
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Item {
            id: posterBox
            Layout.fillWidth: true
            Layout.preferredHeight: Math.round(width * 1.5)
            implicitHeight: Math.round(card.implicitWidth * 1.5)

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.12)
                border.width: 1
            }
            Image {
                id: poster
                anchors.fill: parent
                anchors.margins: 1
                source: card.posterUrl
                    ? "image://kinema/poster?u=" + encodeURIComponent(card.posterUrl)
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                visible: status === Image.Ready
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.large
                height: width
                source: AppIcons.url("film")
                color: Qt.alpha(Theme.foreground, 0.35)
                visible: poster.status !== Image.Ready
            }

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 1
                height: Kirigami.Units.smallSpacing
                radius: height / 2
                color: Qt.alpha(Theme.foreground, 0.18)
                visible: card.progress > 0 && card.progress < 1

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: parent.width * Math.max(0, Math.min(1, card.progress))
                    radius: parent.radius
                    color: Theme.accent
                }
            }

            Rectangle {
                anchors.top: parent.top
                anchors.right: parent.right
                anchors.margins: Kirigami.Units.smallSpacing
                visible: card.watched || card.upcoming
                radius: height / 2
                color: card.watched
                    ? Qt.alpha(Theme.positive, 0.90)
                    : Qt.alpha(Theme.accent, 0.90)
                implicitWidth: badgeRow.implicitWidth + Kirigami.Units.smallSpacing * 2
                implicitHeight: badgeRow.implicitHeight + Kirigami.Units.smallSpacing

                RowLayout {
                    id: badgeRow
                    anchors.centerIn: parent
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon {
                        Layout.preferredWidth: Kirigami.Units.iconSizes.small
                        Layout.preferredHeight: width
                        source: AppIcons.url(card.watched ? "circle-check" : "clock-arrow-down")
                        color: AppIcons.accentText
                    }
                    QQC2.Label {
                        text: card.watched
                            ? i18nc("@label library card badge", "Watched")
                            : i18nc("@label library card badge", "Upcoming")
                        color: AppIcons.accentText
                        font.pointSize: Theme.captionFont.pointSize
                    }
                }
            }
        }

        ColumnLayout {
            id: meta
            Layout.fillWidth: true
            spacing: 0

            QQC2.Label {
                Layout.fillWidth: true
                text: card.title
                maximumLineCount: 2
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                color: Theme.foreground
                font.weight: Font.DemiBold
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: text.length > 0
                text: card.subtitle
                maximumLineCount: 2
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
            }
        }
    }
}
