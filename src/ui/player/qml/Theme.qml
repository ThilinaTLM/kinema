// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

pragma Singleton

import QtQuick

// Tokenised design system for the player chrome. Picks up
// Plasma-leaning defaults and exposes a single source of truth for
// colours, durations, sizes and spacing. QML files reference
// `Theme.<token>` so a future visual pass touches one file.
QtObject {
    // ---- Palette ------------------------------------------------------
    // Dark, slightly translucent surfaces look right against video.
    // Accent picks up Plasma's typical "Breeze" blue but is fixed
    // here so the player chrome reads consistently regardless of
    // the user's desktop accent. Override later if needed.
    readonly property color accent:        "#3DAEE9"
    readonly property color accentBright:  "#61C9F5"
    readonly property color foreground:    "#FFFFFF"
    readonly property color foregroundDim: "#B6BEC9"
    readonly property color background:    "#0D0F12"
    readonly property color surface:       Qt.rgba(0.07, 0.08, 0.10, 0.78)
    readonly property color surfaceElev:   Qt.rgba(0.13, 0.15, 0.19, 0.92)
    readonly property color overlayShade:  Qt.rgba(0.0, 0.0, 0.0, 0.55)
    readonly property color trackOff:      Qt.rgba(1.0, 1.0, 1.0, 0.18)
    readonly property color trackBuffer:   Qt.rgba(1.0, 1.0, 1.0, 0.30)
    readonly property color warning:       "#F67400"
    readonly property color danger:        "#DA4453"

    // ---- Animation ----------------------------------------------------
    readonly property int fadeMs:    180
    readonly property int growMs:    120
    readonly property int popMs:     220
    readonly property int slowMs:    320

    // Chrome auto-hide delay: how long the top bar / transport bar
    // stay visible after the last mouse / wheel / key activity.
    readonly property int chromeAutoHideMs: 1000

    // ---- Sizes / spacing ---------------------------------------------
    // Pixel values are at logical 1:1 scale; QML containers consume
    // them via anchors / margins so HiDPI handling is automatic.
    readonly property int unit:        4
    readonly property int spacing:     8
    readonly property int spacingLg:   16
    readonly property int radius:      8
    readonly property int radiusLg:    14

    readonly property int transportHeight: 92
    readonly property int topBarHeight:    72
    readonly property int seekBarHeight:   6
    readonly property int seekBarHeightHover: 10
    readonly property int iconButton:      40
    readonly property int iconButtonLg:    52

    // ---- Typography --------------------------------------------------
    readonly property string fontFamily: "Sans Serif"
    readonly property int fontSizeSm:  12
    readonly property int fontSize:    14
    readonly property int fontSizeLg:  18
    readonly property int fontSizeXL:  24
}
