// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One TMDB row tile: 2:3 poster image, title, secondary line, and
// a corner rating chip. The hover lift mirrors what
// `Kirigami.AbstractCard` provides on desktop without bringing in a
// full card chrome (which adds a frame line we don't want over the
// poster's own border).
//
// `signal clicked()` is forwarded by the rail's delegate.
Item {
    id: card

    // Inputs.
    property string posterUrl
    property string title
    property string subtitle
    property real rating: -1

    signal clicked()

    // Geometry.
    implicitWidth: Theme.posterMin
    implicitHeight: poster.height + meta.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    // Hover lift via an attached scale; small enough to feel like
    // the desktop card hover state without introducing a layout
    // shift inside a horizontal `ListView`.
    transform: Scale {
        origin.x: card.width / 2
        origin.y: card.height / 2
        xScale: hoverArea.containsMouse ? 1.02 : 1.0
        yScale: hoverArea.containsMouse ? 1.02 : 1.0

        Behavior on xScale { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        Behavior on yScale { NumberAnimation { duration: Kirigami.Units.shortDuration } }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        // ---- Poster ------------------------------------------------
        Item {
            id: poster
            Layout.fillWidth: true
            // 2:3 portrait aspect; clamped so absurdly tall containers
            // (e.g. Show all → grid pages) don't stretch the image.
            Layout.preferredHeight: Math.round(width * 1.5)

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.10)
                border.width: 1
            }

            Image {
                id: posterImage
                anchors.fill: parent
                anchors.margins: 1   // sit inside the border
                source: card.posterUrl
                    ? "image://kinema/poster?u=" + encodeURIComponent(card.posterUrl)
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                sourceSize.width: Theme.posterMax
                sourceSize.height: Math.round(Theme.posterMax * 1.5)
                visible: status === Image.Ready
            }

            // Skeleton + missing-poster fallback. Shows a neutral
            // film icon in the same slot as the image so the rail
            // doesn't acquire empty rectangles for cards whose TMDB
            // row lacked a poster URL.
            Kirigami.Icon {
                visible: posterImage.status !== Image.Ready
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.huge
                height: width
                source: "applications-multimedia"
                color: Qt.alpha(Theme.foreground, 0.35)
            }

            // Rating overlay (top-right). Sits inside the poster's
            // padded inset so it doesn't clip the rounded corner.
            RatingChip {
                rating: card.rating
                anchors {
                    top: parent.top
                    right: parent.right
                    topMargin: Kirigami.Units.smallSpacing
                    rightMargin: Kirigami.Units.smallSpacing
                }
            }
        }

        // ---- Title + secondary line --------------------------------
        ColumnLayout {
            id: meta
            spacing: 0
            Layout.fillWidth: true

            QQC2.Label {
                Layout.fillWidth: true
                text: card.title
                elide: Text.ElideRight
                maximumLineCount: 2
                wrapMode: Text.Wrap
                font.pointSize: Theme.captionFont.pointSize + 1
                font.weight: Font.DemiBold
                color: Theme.foreground
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: card.subtitle.length > 0
                text: card.subtitle
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
            }
        }
    }

    MouseArea {
        id: hoverArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) {
            if (mouse.button === Qt.LeftButton) {
                card.clicked();
            }
        }
    }

    // Subtle focus ring for keyboard nav.
    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        radius: Kirigami.Units.cornerRadius + 2
        color: "transparent"
        border.color: Theme.accent
        border.width: 2
        visible: card.activeFocus
    }
}
