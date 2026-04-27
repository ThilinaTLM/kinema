// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One TMDB / Cinemeta tile: 2:3 poster image, two-line title, a
// caption subtitle and a corner rating chip. Visual chrome leans
// on Kirigami theme tokens so it tracks the user's Plasma colour
// scheme without any hand-rolled palette:
//
//   * `Kirigami.ShadowedImage` renders the artwork through a
//     distance-field shader so the pixels are clipped to the
//     rounded corners (a plain `Image` inside a `ShadowedRectangle`
//     leaks square corners on top of the rounded frame).
//   * The shadow lifts on hover instead of a scale transform —
//     scale would bleed into adjacent grid cells.
//   * `Kirigami.Theme.colorSet: View` so the surface inherits the
//     content area's background, not the page header's.
//   * `hoverColor` / `focusColor` for hover + keyboard focus.
//
// Public surface is intentionally identical to the previous version
// so `PosterGrid`, `ContentRail`, and `SimilarCarousel` can keep
// using the delegate without changes. `ProgressPosterCard` reuses
// the same hover/elevation pattern so the two card variants feel
// like siblings.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
    property string posterUrl
    property string title
    property string subtitle
    property real rating: -1

    signal clicked()

    // Single source of truth for the hover-elevation state so the
    // shadow, border, and tint can all flip together.
    readonly property bool _hovered: hoverHandler.hovered

    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    // Geometry. The grid / rail sets `width`/`height` directly; this
    // keeps a sensible default for ad-hoc consumers.
    implicitWidth: Theme.posterMin
    implicitHeight: poster.height + meta.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    // Keyboard focus support: the parent grid forwards arrow keys
    // through `GridView` and we only need to react to activation.
    activeFocusOnTab: true
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Return
            || event.key === Qt.Key_Enter
            || event.key === Qt.Key_Space) {
            card.clicked();
            event.accepted = true;
        }
    }

    // ---- Visual chrome ------------------------------------------
    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        // Poster frame. `ShadowedImage` renders the artwork through
        // a distance-field shader, so pixels are clipped to the
        // rounded corners — no square overhang at the corners.
        Kirigami.ShadowedImage {
            id: poster
            Layout.fillWidth: true
            // 2:3 portrait aspect, computed from current width so
            // the card stays crisp at every grid density.
            Layout.preferredHeight: Math.round(width * 1.5)

            radius: Kirigami.Units.cornerRadius
            color: Kirigami.Theme.alternateBackgroundColor

            source: card.posterUrl
                ? "image://kinema/poster?u=" + encodeURIComponent(card.posterUrl)
                : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            // Source size derives from the actual width so wide
            // grids don't pull blurry thumbnails. Capped at the
            // theme's `posterMax` to avoid pulling needlessly large
            // TMDB renditions on dense displays. Both dimensions
            // derive from `_srcW` rather than from each other so
            // QQuickImage doesn't see a sourceSize.height ←
            // sourceSize.width binding loop.
            readonly property int _srcW: Math.min(
                card.width * 2, Theme.posterMax * 2)
            sourceSize.width: _srcW
            sourceSize.height: Math.round(_srcW * 1.5)

            border.color: card._hovered || card.activeFocus
                ? Kirigami.Theme.focusColor
                : Qt.alpha(Kirigami.Theme.textColor, 0.12)
            border.width: card.activeFocus ? 2 : 1

            shadow.size: card._hovered
                ? Kirigami.Units.gridUnit
                : Kirigami.Units.smallSpacing
            shadow.yOffset: card._hovered
                ? Kirigami.Units.smallSpacing
                : 1
            shadow.color: Qt.alpha(Kirigami.Theme.textColor,
                card._hovered ? 0.40 : 0.18)

            Behavior on shadow.size {
                NumberAnimation { duration: Kirigami.Units.shortDuration }
            }
            Behavior on shadow.yOffset {
                NumberAnimation { duration: Kirigami.Units.shortDuration }
            }

            // Skeleton + missing-poster fallback. A neutral icon
            // sits in the same slot as the image so cards without
            // a TMDB poster don't render an empty rectangle.
            Kirigami.Icon {
                visible: poster.status !== Image.Ready
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.huge
                height: width
                source: "applications-multimedia"
                color: Kirigami.Theme.disabledTextColor
            }

            // Hover tint, also rounded-corner-clipped via the same
            // distance-field shader so it never escapes the frame.
            Kirigami.ShadowedRectangle {
                anchors.fill: parent
                radius: poster.radius
                color: Kirigami.Theme.hoverColor
                opacity: card._hovered ? 0.18 : 0
                Behavior on opacity {
                    NumberAnimation { duration: Kirigami.Units.shortDuration }
                }
            }

            // Rating overlay (top-right). Sits inside the inset so
            // it does not clip the rounded corner.
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

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 5
                text: card.title
                elide: Text.ElideRight
                maximumLineCount: 1
                color: Kirigami.Theme.textColor
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: card.subtitle.length > 0
                text: card.subtitle
                elide: Text.ElideRight
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    // Click + hover handlers. TapHandler covers click + keyboard
    // synthesised activation via `Keys.onPressed` above.
    TapHandler {
        acceptedButtons: Qt.LeftButton
        onTapped: card.clicked()
    }
    HoverHandler {
        id: hoverHandler
        cursorShape: Qt.PointingHandCursor
    }
}
