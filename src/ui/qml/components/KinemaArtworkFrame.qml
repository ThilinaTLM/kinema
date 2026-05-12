// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Shared artwork chrome used by every poster/thumbnail card in the
// app: PosterCard (Discover/Browse/Search), LibraryCard (Library
// list view), and EpisodeRailCard (Up Next rails, including
// Continue Watching).
//
// What the frame owns:
//   * a `Kirigami.ShadowedImage` rendered through a distance-field
//     shader, so the artwork is clipped to the rounded corners
//     cleanly and the drop shadow can lift on hover without
//     bleeding into adjacent grid cells,
//   * an animated border that switches to `Kirigami.Theme.focusColor`
//     when the parent card is hovered or focused,
//   * a hover tint rendered through an inner ShadowedRectangle so it
//     stays clipped to the same radius,
//   * a fallback icon centred over the alt-bg when the image is not
//     yet ready or fails to load,
//   * an optional progress bar overlay along the bottom inset (the
//     same translucent track + highlight fill used everywhere).
//
// What the frame does NOT own:
//   * input handlers (TapHandler/MouseArea/Keys) — each card wires
//     its own activation semantics so left-click vs right-click vs
//     keyboard activation can vary,
//   * chips and badges — exposed via the `default property alias`
//     slot so the card composes its own RatingChip / StatusChip.
//
// State (`hovered`, `focusRing`) is driven by the parent so the
// card owns the truth about hover/focus and can combine multiple
// inputs (HoverHandler.hovered, MouseArea.containsMouse,
// activeFocus, etc.) into a single boolean.
Item {
    id: frame

    // ---- Inputs --------------------------------------------------
    /// Raw http(s) artwork URL. Routed through the kinema image
    /// provider so caching + 2x source-size capping work the same
    /// way for everyone.
    property string url: ""

    /// Image-provider role. Maps to `image://kinema/<role>?u=…`.
    /// `poster` (default) for 2:3 covers, `still` for 16:9 episode
    /// stills, `backdrop` for hero art.
    property string imageRole: "poster"

    /// Aspect ratio expressed as `height / width`. 1.5 = 2:3
    /// poster, 9/16 = 16:9 episode still.
    property real aspect: 1.5

    /// `false` → `PreserveAspectCrop` (poster fills frame, episode
    /// stills crop into a 16:9 frame). `true` → `PreserveAspectFit`
    /// (used when a 2:3 poster is letterboxed into a 16:9 frame as
    /// a thumbnail-fallback).
    property bool letterbox: false

    /// Icon shown over the alt-bg when `image` is not Ready.
    property string fallbackIcon: "film"

    /// Hover lift state — driven by the parent card.
    property bool hovered: false

    /// Focus ring — when true, border swaps to focusColor + 2 px.
    /// Cards typically bind this to `_hovered || activeFocus`.
    property bool focusRing: false

    /// Built-in progress bar overlay along the bottom inset. `< 0`
    /// or `>= 1` hides the bar entirely.
    property real progress: -1

    /// Slot for chips/badges. Children are placed on top of the
    /// image, inside the rounded frame.
    default property alias overlays: overlayHost.data

    // ---- Outputs -------------------------------------------------
    /// Forwarded when the inner Image hits `Image.Error`. Used by
    /// EpisodeRailCard to advance its thumbnail → poster fallback
    /// chain without owning the Image itself.
    signal imageError()

    // The View colorSet pins the alt-bg / textColor / hoverColor
    // tokens to the content area (not the page header), regardless
    // of where the card is hosted.
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    implicitWidth: Theme.posterMin
    implicitHeight: Math.round(implicitWidth * aspect)

    Kirigami.ShadowedImage {
        id: image
        anchors.fill: parent

        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.alternateBackgroundColor

        source: frame.url.length > 0
            ? "image://kinema/" + frame.imageRole
                + "?u=" + encodeURIComponent(frame.url)
            : ""
        fillMode: frame.letterbox
            ? Image.PreserveAspectFit
            : Image.PreserveAspectCrop
        asynchronous: true

        // Cap source size at twice the rendered width so dense
        // grids don't fetch needlessly large TMDB renditions.
        // Both dimensions derive from `_srcW` rather than from each
        // other so QQuickImage doesn't see a binding loop on
        // sourceSize.
        readonly property int _srcW: Math.min(
            frame.width * 2, Theme.posterMax * 2)
        sourceSize.width: _srcW
        sourceSize.height: Math.round(_srcW * frame.aspect)

        border.color: frame.focusRing
            ? Kirigami.Theme.focusColor
            : Qt.alpha(Kirigami.Theme.textColor, 0.12)
        border.width: frame.focusRing ? 2 : 1

        shadow.size: frame.hovered
            ? Kirigami.Units.gridUnit
            : Kirigami.Units.smallSpacing
        shadow.yOffset: frame.hovered
            ? Kirigami.Units.smallSpacing
            : 1
        shadow.color: Qt.alpha(Kirigami.Theme.textColor,
            frame.hovered ? 0.40 : 0.18)

        Behavior on shadow.size {
            NumberAnimation { duration: Kirigami.Units.shortDuration }
        }
        Behavior on shadow.yOffset {
            NumberAnimation { duration: Kirigami.Units.shortDuration }
        }

        onStatusChanged: {
            if (status === Image.Error) {
                frame.imageError();
            }
        }

        // Skeleton + missing-art fallback. Sits in the same slot as
        // the image so cards without artwork don't render an empty
        // rectangle.
        Kirigami.Icon {
            visible: image.status !== Image.Ready
            anchors.centerIn: parent
            width: Kirigami.Units.iconSizes.huge
            height: width
            source: AppIcons.url(frame.fallbackIcon)
            color: Kirigami.Theme.disabledTextColor
        }

        // Hover tint, rounded-corner-clipped via the same
        // distance-field shader so it never escapes the frame.
        Kirigami.ShadowedRectangle {
            anchors.fill: parent
            radius: image.radius
            color: Kirigami.Theme.hoverColor
            opacity: frame.hovered ? 0.18 : 0
            Behavior on opacity {
                NumberAnimation { duration: Kirigami.Units.shortDuration }
            }
        }

        // Default-property slot for card-specific overlays
        // (RatingChip, StatusChip, kebab buttons, …).
        // Anchored to the image so children can position themselves
        // with anchors against the frame's edges.
        Item {
            id: overlayHost
            anchors.fill: parent
        }

        // Progress bar overlay along the bottom inset. The track
        // sits inside the rounded corner inset so it doesn't clip
        // the rounded corners. Style is fixed app-wide: translucent
        // backdrop + theme highlight fill.
        Item {
            visible: frame.progress > 0 && frame.progress < 1
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Kirigami.Units.smallSpacing
                rightMargin: Kirigami.Units.smallSpacing
                bottomMargin: Kirigami.Units.smallSpacing
            }
            height: 4

            Rectangle {
                anchors.fill: parent
                radius: 2
                color: Qt.alpha(Kirigami.Theme.backgroundColor, 0.62)
            }

            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: parent.width
                    * Math.max(0, Math.min(1, frame.progress))
                radius: 2
                color: Kirigami.Theme.highlightColor
            }
        }
    }
}
