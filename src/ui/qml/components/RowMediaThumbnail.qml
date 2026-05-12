// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Thin wrapper over `KinemaArtworkFrame` for the leading thumbnail
// slot in `EpisodeListCard` (16:9 still) and `DownloadListCard`
// (2:3 poster). Replaces the two inline
// `Item { Rectangle + Image + Kirigami.Icon }` stacks the rows used
// to ship, so caching and 2x source-size capping flow through the
// shared image provider exactly like every other artwork surface.
//
// Sizes itself off `Layout.preferredWidth/Height` from the parent;
// the inner frame fills the wrapper.
Item {
    id: thumb

    // ---- Inputs --------------------------------------------------
    property string url
    property string fallbackIcon: "play"
    /// `still` for 16:9 episode thumbnails, `poster` for 2:3 covers,
    /// `backdrop` for hero art. Routed through
    /// `image://kinema/<role>?u=…` so the existing provider cache
    /// is reused.
    property string imageRole: "poster"
    /// Hover lift, forwarded by the parent row so the frame's
    /// shadow + tint stays in sync with the row's hover state.
    property bool hovered: false
    /// Aspect ratio (height / width). 9/16 for stills, 1.5 for
    /// posters. Cards typically set Layout.preferredWidth/Height to
    /// the intended pixel size; this value matches the
    /// `KinemaArtworkFrame` aspect for the in-image fallback icon
    /// sizing.
    property real aspect: 1.5

    // ---- Slot ----------------------------------------------------
    /// Overlay slot forwarded to the artwork frame (pin badge,
    /// duration chip, etc.). Children anchor against `parent`,
    /// which resolves to the frame.
    default property alias overlays: frame.overlays

    implicitWidth: Theme.posterMin
    implicitHeight: Math.round(implicitWidth * aspect)

    KinemaArtworkFrame {
        id: frame
        anchors.fill: parent
        url: thumb.url
        imageRole: thumb.imageRole
        aspect: thumb.aspect
        fallbackIcon: thumb.fallbackIcon
        hovered: thumb.hovered
        focusRing: false
    }
}
