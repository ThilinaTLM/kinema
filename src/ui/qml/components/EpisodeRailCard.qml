// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Card for the Library page's episode-flavored rails (Up Next /
// Airing Soon). Compact 16:9 thumbnail with two text lines:
//
//   primaryLine    \u2014 e.g. "S02E08 \u2014 The Bell"
//   secondaryLine  \u2014 e.g. "Severance"  /  "Airs Fri, Apr 26"
//
// Falls back to the parent series poster (2:3) when no episode
// thumbnail is available so empty thumbnails don't manifest as
// gaps. Optional progress bar over the bottom edge for Up Next
// rows that carry resume progress.
//
// Public surface mirrors `PosterCard`: `clicked()` for activation,
// no internal context menu (the rails do not currently expose one).
QQC2.ItemDelegate {
    id: card

    property string posterUrl: ""
    property string thumbnailUrl: ""
    property string primaryLine: ""
    property string secondaryLine: ""
    property real progress: -1
    /// Artwork aspect ratio (height / width). Defaults to 9/16 so
    /// the card renders a wide episode still. The Coming Up movie
    /// rail overrides this with `1.5` (2:3) so the same component
    /// can host poster artwork without distorting it via crop.
    property real artworkAspect: 9 / 16
    /// Fallback icon shown while the artwork is loading or when
    /// neither URL is set.
    property string fallbackIcon: "tv"

    padding: 0
    implicitWidth: Kirigami.Units.gridUnit * 16
    implicitHeight: art.implicitHeight + meta.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: card.hovered
            ? Kirigami.Theme.alternateBackgroundColor
            : "transparent"
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Item {
            id: art
            Layout.fillWidth: true
            // Artwork height derived from the configured aspect.
            // 9/16 = wide episode still; 1.5 = 2:3 poster.
            Layout.preferredHeight: Math.round(width * card.artworkAspect)
            implicitHeight: Math.round(card.implicitWidth
                * card.artworkAspect)

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.12)
                border.width: 1
            }
            Image {
                id: art_image
                anchors.fill: parent
                anchors.margins: 1
                source: {
                    const u = card.thumbnailUrl.length > 0
                        ? card.thumbnailUrl : card.posterUrl;
                    return u.length > 0
                        ? "image://kinema/poster?u=" + encodeURIComponent(u)
                        : "";
                }
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                visible: status === Image.Ready
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.large
                height: width
                source: AppIcons.url(card.fallbackIcon)
                color: Qt.alpha(Theme.foreground, 0.35)
                visible: art_image.status !== Image.Ready
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
                    width: parent.width
                        * Math.max(0, Math.min(1, card.progress))
                    radius: parent.radius
                    color: Theme.accent
                }
            }
        }

        ColumnLayout {
            id: meta
            Layout.fillWidth: true
            spacing: 0

            QQC2.Label {
                Layout.fillWidth: true
                text: card.primaryLine
                maximumLineCount: 1
                elide: Text.ElideRight
                color: Theme.foreground
                font.weight: Font.DemiBold
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: text.length > 0
                text: card.secondaryLine
                maximumLineCount: 1
                elide: Text.ElideRight
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
            }
        }
    }
}
