// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Card for the Library page's smart rails. Sibling of `PosterCard`
// and `LibraryCard` — `KinemaArtworkFrame` owns the shadow lift,
// focus ring, hover tint, and rounded-corner clipping.
//
// Two artwork modes, picked by `artworkAspect`:
//
//   * 9 / 16  — wide episode still cropped into the frame,
//   * 1.5     — 2:3 movie poster, fills the frame.
//
// And up to three meta lines:
//
//   primaryLine    — show or movie title
//   secondaryLine  — episode designator ("S02E08 \u00b7 The Bell")
//   tertiaryLine   — meta line ("Airs Fri, Apr 26", "Resume from 42%")
//
// Empty lines are hidden so a movie row (two lines) and an episode
// row (three lines) coexist without forcing extra whitespace.
//
// Artwork resolves through a `thumbnail \u2192 poster \u2192
// fallback icon` chain. The card starts on whichever URL is
// supplied and advances on `KinemaArtworkFrame.imageError()`, so
// an unaired episode whose Cinemeta thumbnail 404s still ends up
// showing the parent series poster instead of an empty frame.
// When the card is configured with a 16:9 frame but resolves to a
// 2:3 poster, the frame switches to letterbox mode so the poster
// sits letterboxed instead of being crop-distorted.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
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

    signal clicked()

    /// Artwork resolution step:
    ///   0 — try `thumbnailUrl` (cropped 16:9 episode still)
    ///   1 — try `posterUrl`    (letterboxed 2:3 poster fallback)
    ///   2 — give up, render the fallback icon over the alt-bg
    ///
    /// Initial step is computed from which URL is non-empty; the
    /// frame's `imageError` signal advances on load errors, and
    /// the `on*UrlChanged` handlers reset the step when the
    /// delegate is recycled to a different model row.
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

    readonly property bool _hovered: hoverHandler.hovered

    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    implicitWidth: Kirigami.Units.gridUnit * 16
    // Compute implicit height from the *actual* width. Both call
    // sites (LibraryRail delegate + visible:false prototype) set
    // `width: rail.cardWidth` explicitly, and `Item.width` already
    // defaults to `implicitWidth` when no binding is assigned, so
    // the previous `width > 0 ? width : implicitWidth` fallback was
    // dead code that introduced an extra dependency on
    // `implicitWidth` and triggered a binding-loop warning during
    // initial sizing.
    implicitHeight: Math.round(width * artworkAspect)
        + meta.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    activeFocusOnTab: true
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Return
            || event.key === Qt.Key_Enter
            || event.key === Qt.Key_Space) {
            card.clicked();
            event.accepted = true;
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        KinemaArtworkFrame {
            id: art
            Layout.fillWidth: true
            Layout.preferredHeight: Math.round(width * card.artworkAspect)
            // Mirror the card's resolved width so the implicit
            // height of the frame matches the laid-out frame even
            // before `Layout.preferredHeight` settles.
            implicitHeight: Math.round(card.width
                * card.artworkAspect)

            url: card._artUrl
            aspect: card.artworkAspect
            // Real episode still → crop. Poster fallback in a 16:9
            // frame → letterbox so the 2:3 art isn't stretched.
            // Posters in their native aspect (1.5) crop neutrally,
            // so the flag only matters for the thumbnail-fallback
            // case.
            letterbox: !card._artIsThumbnail
                && card.artworkAspect < 1
            fallbackIcon: card.fallbackIcon
            hovered: card._hovered
            focusRing: card._hovered || card.activeFocus
            progress: card.progress

            // Walk the fallback chain on load failure: thumbnail →
            // poster → give up. Cinemeta sometimes returns a
            // thumbnail URL for unaired episodes whose still
            // doesn't actually exist on the CDN yet, and without
            // this hop the frame would just stay empty.
            onImageError: {
                if (card._artStep === 0
                    && card.posterUrl.length > 0
                    && card.posterUrl !== card.thumbnailUrl) {
                    card._artStep = 1;
                } else if (card._artStep < 2) {
                    card._artStep = 2;
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

    TapHandler {
        acceptedButtons: Qt.LeftButton
        onTapped: card.clicked()
    }
    HoverHandler {
        id: hoverHandler
        cursorShape: Qt.PointingHandCursor
    }
}
