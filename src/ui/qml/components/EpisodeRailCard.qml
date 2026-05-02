// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Card for the Library page's smart rails.
//
// Renders artwork (16:9 episode still or 2:3 movie poster, picked
// by `artworkAspect`) plus up to three text lines:
//
//   primaryLine    — show or movie title
//   secondaryLine  — episode designator ("S02E08 · The Bell")
//   tertiaryLine   — meta line ("Airs Fri, Apr 26", "Resume from 42%")
//
// Empty lines are hidden so a movie row (two lines) and an episode
// row (three lines) coexist without forcing extra whitespace.
//
// Artwork resolves through a `thumbnail → poster → fallback icon`
// chain. The card starts on whichever URL is supplied and advances
// on `Image.Error`, so an unaired episode whose Cinemeta thumbnail
// 404s still ends up showing the parent series poster instead of
// an empty frame. When the card is configured with a 16:9 frame but
// resolves to a 2:3 poster (either as the initial source or after
// an error retry), `fillMode` switches to `PreserveAspectFit` so
// the poster sits letterboxed instead of being crop-distorted.
QQC2.ItemDelegate {
    id: card

    property string posterUrl: ""
    property string thumbnailUrl: ""
    property string primaryLine: ""
    property string secondaryLine: ""
    property string tertiaryLine: ""
    property real progress: -1
    /// Artwork aspect ratio (height / width). 9/16 = wide episode
    /// still, 1.5 = 2:3 poster.
    property real artworkAspect: 9 / 16
    /// Fallback icon when no artwork is available at all.
    property string fallbackIcon: "tv"

    /// Artwork resolution step:
    ///   0 — try `thumbnailUrl` (cropped 16:9 episode still)
    ///   1 — try `posterUrl`    (letterboxed 2:3 poster fallback)
    ///   2 — give up, render the fallback icon over the alt-bg
    ///
    /// Initial step is computed from which URL is non-empty; the
    /// `Image.onStatusChanged` handler advances on load errors. The
    /// `on*UrlChanged` handlers reset the step when the delegate is
    /// recycled to a different model row.
    property int _artStep: thumbnailUrl.length > 0
        ? 0 : (posterUrl.length > 0 ? 1 : 2)

    readonly property string _artUrl:
        _artStep === 0 ? thumbnailUrl
        : _artStep === 1 ? posterUrl
        : ""

    /// Step 0 = real episode still → crop into the rail's frame.
    /// Steps 1/2 = poster fallback → letterbox so 2:3 art doesn't
    /// stretch inside a 16:9 frame.
    readonly property bool _artIsThumbnail: _artStep === 0

    onThumbnailUrlChanged: _artStep =
        thumbnailUrl.length > 0
            ? 0 : (posterUrl.length > 0 ? 1 : 2)
    onPosterUrlChanged: {
        if (_artStep === 1 && posterUrl.length === 0) {
            _artStep = 2;
        } else if (_artStep === 2 && posterUrl.length > 0
            && thumbnailUrl.length === 0) {
            _artStep = 1;
        }
    }

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
                id: artImage
                anchors.fill: parent
                anchors.margins: 1
                source: card._artUrl.length > 0
                    ? "image://kinema/poster?u="
                        + encodeURIComponent(card._artUrl)
                    : ""
                // Crop episode thumbnails into the 16:9 frame for
                // visual density; letterbox poster fallbacks so a
                // 2:3 source isn't stretched ugly.
                fillMode: card._artIsThumbnail
                    ? Image.PreserveAspectCrop
                    : Image.PreserveAspectFit
                asynchronous: true
                cache: true
                visible: status === Image.Ready

                // Walk the fallback chain on load failure: thumbnail
                // → poster → give up. Cinemeta sometimes returns a
                // thumbnail URL for unaired episodes whose still
                // doesn't actually exist on the CDN yet, and without
                // this hop the frame would just stay empty.
                onStatusChanged: {
                    if (status !== Image.Error) {
                        return;
                    }
                    if (card._artStep === 0
                        && card.posterUrl.length > 0
                        && card.posterUrl !== card.thumbnailUrl) {
                        card._artStep = 1;
                    } else {
                        card._artStep = 2;
                    }
                }
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.large
                height: width
                source: AppIcons.url(card.fallbackIcon)
                color: Qt.alpha(Theme.foreground, 0.35)
                visible: artImage.status !== Image.Ready
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
                color: Theme.foreground
                font.pointSize: Theme.captionFont.pointSize
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: text.length > 0
                text: card.tertiaryLine
                maximumLineCount: 1
                elide: Text.ElideRight
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
            }
        }
    }
}
