// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

pragma Singleton

import QtQuick

// Tokenised design system for the player chrome. Mirrors the KDE
// Breeze Dark palette and Plasma sizing conventions so panels feel
// at home for users running Breeze Dark, while staying fixed-dark
// regardless of the active KDE colour scheme — a light popup over a
// video frame is unreadable in bright scenes. QML files reference
// `Theme.<token>` so a future visual pass touches one file.
QtObject {
    // ---- Palette (Breeze Dark exact values) --------------------------
    // - accent / hover / focus: rgb(61, 174, 233) — Breeze Dark's
    //   DecorationFocus / DecorationHover / Selection background.
    // - background / view: Breeze Dark Window/View backgrounds.
    // - foreground: Breeze Dark Window foreground.
    readonly property color accent:        "#3DAEE9"
    readonly property color accentHover:   "#61C9F5"
    readonly property color accentPressed: "#2D8FBF"
    readonly property color foreground:    "#FCFCFC"
    readonly property color foregroundDim: "#B8BCC1"
    readonly property color background:    "#202326"
    readonly property color view:          "#141618"
    readonly property color surface:       Qt.rgba(0.125, 0.137, 0.149, 0.82)
    readonly property color surfaceElev:   Qt.rgba(0.161, 0.173, 0.188, 0.96)
    readonly property color border:        Qt.rgba(1.0, 1.0, 1.0, 0.10)
    readonly property color overlayShade:  Qt.rgba(0.0, 0.0, 0.0, 0.55)
    readonly property color trackOff:      Qt.rgba(1.0, 1.0, 1.0, 0.18)
    readonly property color trackBuffer:   Qt.rgba(1.0, 1.0, 1.0, 0.30)
    readonly property color warning:       "#F67400"
    readonly property color danger:        "#DA4453"

    // ---- Hover / selected fills (Breeze conventions) -----------------
    // Plasma's qqc2-desktop-style uses Qt.alpha(hoverColor, 0.3) for
    // hover and the full highlight colour for selection, with white
    // (highlightedText) text on the selected pill.
    readonly property color hoverFill:       Qt.rgba(0.239, 0.682, 0.914, 0.30)
    readonly property color selectedFill:    accent
    readonly property color highlightedText: "#FCFCFC"

    // ---- Animation ----------------------------------------------------
    // Aligned to Plasma's Kirigami.Units durations (short ≈ 100,
    // long ≈ 200). The old chrome used 180/220 for hover transitions
    // which felt sluggish next to native menus.
    readonly property int fadeMs:    100
    readonly property int growMs:    120
    readonly property int popMs:     200
    readonly property int slowMs:    320

    // Chrome auto-hide delay: how long the top bar / transport bar
    // stay visible after the last mouse / wheel / key activity.
    readonly property int chromeAutoHideMs: 1000

    // ---- Sizes / spacing ---------------------------------------------
    // Pixel values are at logical 1:1 scale; QML containers consume
    // them via anchors / margins so HiDPI handling is automatic.
    // Plasma maps small=4 / medium=6 / large=8 / huge=16. We keep
    // `spacing` at 8 to preserve current layouts; new code can reach
    // for spacingXs / spacingSm / spacingLg when finer control is
    // needed.
    readonly property int unit:        4
    readonly property int spacingXs:   4
    readonly property int spacingSm:   6
    readonly property int spacing:     8
    readonly property int spacingLg:   16

    // Plasma cornerRadius is 5 px; popup panels get a slightly larger
    // 8 px to sit comfortably in fullscreen video chrome.
    readonly property int radius:      5
    readonly property int radiusLg:    8

    readonly property int transportHeight: 92
    readonly property int topBarHeight:    72
    readonly property int seekBarHeight:   6
    readonly property int seekBarHeightHover: 10
    readonly property int iconButton:      40
    readonly property int iconButtonLg:    52

    // ---- Drop shadow tokens (PopupPanel, MultiEffect) ----------------
    readonly property int   popupShadowSize:   16
    readonly property int   popupShadowOffset: 4
    readonly property color popupShadowColor:  Qt.rgba(0.0, 0.0, 0.0, 0.55)

    // ---- Typography --------------------------------------------------
    // Inherit the user's KDE "General font" via Qt.application.font.
    // The KDE platform integration plugin populates that from
    // ~/.config/kdeglobals [General].font, so reading it here picks
    // up the same family + point size the user configured in System
    // Settings → Fonts. Sizes are exposed as point sizes so they
    // scale with the user's screen DPI / accessibility settings;
    // hardcoded pixelSize is avoided everywhere in the chrome.
    //
    // Call sites usually want one of the prebuilt font groups
    // (`defaultFont` / `smallFont` / `titleFont` / `headerFont` /
    // `monoFont`). For one-off combos the raw `fontPoint*` values
    // can be paired with a per-element `font.weight` /
    // `font.capitalization` override.
    readonly property real fontPointSm:
        Math.max(7, Qt.application.font.pointSize - 1)
    readonly property real fontPoint:
        Qt.application.font.pointSize
    readonly property real fontPointLg:
        Qt.application.font.pointSize + 1
    readonly property real fontPointXL:
        Qt.application.font.pointSize + 4

    readonly property font defaultFont: Qt.application.font

    // Small caption text (chips, hints, secondary rows). Equivalent
    // to Plasma's `smallestReadableFont` heuristic: general - 1 pt.
    readonly property font smallFont: ({
        family:    Qt.application.font.family,
        pointSize: Math.max(7, Qt.application.font.pointSize - 1)
    })

    // Title strip (media title in the top bar). Slight uplift over
    // the body font, DemiBold for hierarchy.
    readonly property font titleFont: ({
        family:    Qt.application.font.family,
        pointSize: Qt.application.font.pointSize + 1,
        weight:    Font.DemiBold
    })

    // Panel headers ("Player info", "Resume playback?", picker
    // titles). Larger and DemiBold for visual hierarchy inside
    // PopupPanel.
    readonly property font headerFont: ({
        family:    Qt.application.font.family,
        pointSize: Qt.application.font.pointSize + 3,
        weight:    Font.DemiBold
    })

    // Monospace for timecodes, codec names, source URL. "Monospace"
    // is Qt's logical family that resolves to the system's chosen
    // fixed-width font on KDE.
    readonly property font monoFont: ({
        family:    "Monospace",
        pointSize: Qt.application.font.pointSize
    })
    readonly property font monoSmallFont: ({
        family:    "Monospace",
        pointSize: Math.max(7, Qt.application.font.pointSize - 1)
    })
}
