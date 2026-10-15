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
// Emits two activation signals: `clicked()` for left-click /
// keyboard activation, and `rightClicked()` for the optional
// right-click affordance used by Continue Watching to surface its
// context menu. Other consumers can ignore `rightClicked()`.
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
// Artwork resolves through a `thumbnail \u2192 backdrop \u2192
// poster \u2192 fallback icon` chain. The card starts on whichever
// URL is supplied and advances on
// `KinemaArtworkFrame.imageError()`, so an unaired episode whose
// Cinemeta thumbnail 404s falls back to the parent show's hero
// backdrop (same image used on the detail page) instead of an
// empty frame. The 2:3 poster is the last-resort artwork for the
// rare case where TMDB lacks a backdrop; in a 16:9 frame it's
// rendered letterboxed instead of being crop-distorted.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
    property string posterUrl: ""
    property string backdropUrl: ""
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
    signal rightClicked()

    /// Artwork resolution step:
    ///   0 — try `thumbnailUrl` (cropped 16:9 episode still)
    ///   1 — try `backdropUrl`  (cropped 16:9 show backdrop)
    ///   2 — try `posterUrl`    (letterboxed 2:3 show poster)
    ///   3 — give up, render the fallback icon over the alt-bg
    ///
    /// Initial step skips past empty URLs; the frame's `imageError`
    /// signal advances on load errors, and the `on*UrlChanged`
    /// handlers reset the step when the delegate is recycled.
    function _initialStep() {
        if (thumbnailUrl.length > 0) return 0;
        if (backdropUrl.length > 0) return 1;
        if (posterUrl.length > 0) return 2;
        return 3;
    }

    property int _artStep: _initialStep()

    readonly property string _artUrl:
        _artStep === 0 ? thumbnailUrl
        : _artStep === 1 ? backdropUrl
        : _artStep === 2 ? posterUrl
        : ""

    /// Steps 0/1 are 16:9 artwork (episode still / show backdrop)
    /// — crop into the rail's frame. Step 2 is the 2:3 poster
    /// fallback — letterbox inside a 16:9 frame so the art isn't
    /// crop-distorted.
    readonly property bool _artIsLetterbox:
        _artStep === 2 && card.artworkAspect < 1

    onThumbnailUrlChanged: _artStep = _initialStep()
    onBackdropUrlChanged: _artStep = _initialStep()
    onPosterUrlChanged: _artStep = _initialStep()

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
    // Keyboard menu key (Shift+F10 / Menu) opens the context menu
    // via the same `rightClicked()` signal that mouse right-click
    // uses, so keyboard-only users reach the same menu as mouse
    // users.
    Keys.onMenuPressed: function (event) {
        card.rightClicked();
        event.accepted = true;
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
            // 16:9 artwork (steps 0/1) crops into the frame. 2:3
            // poster (step 2) inside a 16:9 frame letterboxes so
            // the art isn't crop-distorted. Native-aspect posters
            // in poster-aspect frames (Recently Added) crop
            // neutrally and never reach the letterbox path.
            letterbox: card._artIsLetterbox
            fallbackIcon: card.fallbackIcon
            hovered: card._hovered
            focusRing: card._hovered || card.activeFocus
            progress: card.progress

            // Walk the fallback chain on load failure:
            //   thumbnail → backdrop → poster → give up.
            // Skip steps whose URL is empty (or duplicates an
            // already-failed URL) so we don't bounce between
            // identical sources.
            onImageError: {
                let next = card._artStep + 1;
                while (next <= 2) {
                    const url = next === 1
                        ? card.backdropUrl
                        : card.posterUrl;
                    if (url.length > 0
                        && url !== card._artUrl) {
                        break;
                    }
                    next += 1;
                }
                card._artStep = next;
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
    // Right-click is opt-in: consumers that want a context menu
    // (Continue Watching) connect to `rightClicked()`; the other
    // rails leave it unconnected and right-click is a no-op.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: card.rightClicked()
    }
    HoverHandler {
        id: hoverHandler
        cursorShape: Qt.PointingHandCursor
    }
}
