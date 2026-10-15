// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

pragma Singleton

import QtQuick
import org.kde.kirigami as Kirigami

// Thin Kirigami façade for the player chrome. Sizes derive from
// `Kirigami.Units` so layouts scale with the user's KDE font / DPI
// settings; colours derive from `Kirigami.Theme` with the singleton
// pinned to the Header colorSet (`titlebar` palette). Header is dark
// in every stock Breeze variant (Breeze, Breeze Dark, Breeze
// Twilight) and accent-tinted in custom schemes — chrome therefore
// follows the system theme without producing light popups over
// bright video frames. Per-overlay roots also set
// `Kirigami.Theme.colorSet = Header; inherit = false` so child
// QQC2 controls (TextField, ComboBox, Button …) inherit the same
// register; together with `QQuickStyle::setStyle("org.kde.desktop")`
// in main.cpp this means stock controls render with Breeze and
// match the bespoke chrome around them.
QtObject {
    id: theme

    // Pin the singleton to the titlebar palette. Children that
    // reference `Theme.foreground` / `Theme.background` etc. resolve
    // through this object, so the override propagates.
    Kirigami.Theme.colorSet: Kirigami.Theme.Header
    Kirigami.Theme.inherit: false

    // QML lacks a built-in "same colour, different alpha" helper —
    // `Qt.alpha()` doesn't exist, `Qt.rgba()` requires r/g/b. This
    // stamps alpha onto a `color` value while preserving its r/g/b.
    function withAlpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a); }

    // ---- Palette --------------------------------------------------
    // Slot mapping: Kirigami.Theme.* with the Header colorSet active.
    // Translucent variants (`surface`, `surfaceElev`, `border`,
    // `hoverFill`) keep alpha-mixed versions of the system colours
    // so panels still feel layered over the video frame.
    // Track-specific palette (`trackPlayed`, `trackRest`,
    // `trackBufferBg`, `trackBufferDot`, plus the matching text
    // colours) lives further down and is shared between SeekBar
    // and VolumeControl.
    readonly property color accent:        Kirigami.Theme.highlightColor
    readonly property color accentHover:
        Qt.lighter(Kirigami.Theme.highlightColor, 1.15)
    readonly property color accentPressed:
        Qt.darker(Kirigami.Theme.highlightColor, 1.15)
    readonly property color foreground:    Kirigami.Theme.textColor
    readonly property color foregroundDim: Kirigami.Theme.disabledTextColor
    readonly property color background:    Kirigami.Theme.backgroundColor
    readonly property color surface:
        theme.withAlpha(Kirigami.Theme.backgroundColor, 0.82)
    readonly property color surfaceElev:
        theme.withAlpha(Kirigami.Theme.backgroundColor, 0.96)
    readonly property color border:
        theme.withAlpha(Kirigami.Theme.textColor, 0.10)
    readonly property color hoverFill:
        theme.withAlpha(Kirigami.Theme.highlightColor, 0.30)
    readonly property color selectedFill:    Kirigami.Theme.highlightColor
    readonly property color highlightedText: Kirigami.Theme.highlightedTextColor
    readonly property color danger:          Kirigami.Theme.negativeTextColor

    // ---- Track palette (SeekBar + VolumeControl) ----------------
    // Plex/Netflix-ish: solid white played, dim base under tiled
    // dots for the buffered span, lower-alpha solid grey for the
    // un-buffered remainder. Fixed white is intentional — keeps the
    // chrome neutral over arbitrary frames; system accent would
    // fight bright video. Inside-track text colours are paired so
    // labels read above either the white played span or the dim
    // rest via dual-colour clipping in the track components.
    readonly property color trackPlayed:      "#FFFFFF"
    readonly property color trackRest:
        theme.withAlpha(Kirigami.Theme.textColor, 0.10)
    readonly property color trackBufferBg:
        theme.withAlpha(Kirigami.Theme.textColor, 0.18)
    readonly property color trackBufferDot:
        Qt.rgba(1.0, 1.0, 1.0, 0.55)
    // Translucent pill behind in-bar labels (SeekBar time labels,
    // VolumeControl volume number). Dark on every theme so the
    // white text on top reads clearly regardless of what's under
    // it — played span, dim rest, chapter tick, or a chapter
    // boundary in the seek bar.
    readonly property color trackLabelBg:      Qt.rgba(0.0, 0.0, 0.0, 0.55)

    // Backdrop dim under modal overlays. Not a theme colour — always
    // pure black at 55 % alpha so it reads against any video frame.
    readonly property color overlayShade:    Qt.rgba(0.0, 0.0, 0.0, 0.55)

    // ---- Animation ------------------------------------------------
    // Aligned to Kirigami.Units durations.
    readonly property int fadeMs:    Kirigami.Units.shortDuration
    readonly property int growMs:
        Math.round((Kirigami.Units.shortDuration
                    + Kirigami.Units.longDuration) / 2.5)
    readonly property int popMs:     Kirigami.Units.longDuration
    readonly property int slowMs:
        Math.round(Kirigami.Units.longDuration * 1.6)

    // Chrome auto-hide delay: how long the top bar / transport bar
    // stay visible after the last mouse / wheel / key activity. Not
    // a Plasma token — UX timing.
    readonly property int chromeAutoHideMs: 1000

    // ---- Sizes / spacing -----------------------------------------
    // Re-derived from Kirigami.Units. `gridUnit` and
    // `compactBreakpoint` are exposed for overlay roots that need
    // to compute responsive layout.
    readonly property int gridUnit:           Kirigami.Units.gridUnit
    readonly property int compactBreakpoint:  Kirigami.Units.gridUnit * 30

    readonly property int unit:        Kirigami.Units.smallSpacing
    readonly property int spacingXs:   Kirigami.Units.smallSpacing
    readonly property int spacingSm:
        Math.round(Kirigami.Units.smallSpacing * 1.5)
    readonly property int spacing:     Kirigami.Units.largeSpacing
    readonly property int spacingLg:   Kirigami.Units.largeSpacing * 2

    readonly property int radius:      Kirigami.Units.cornerRadius
    readonly property int radiusLg:
        Kirigami.Units.cornerRadius + Kirigami.Units.smallSpacing

    // Player-specific structural sizes — all expressed in grid units
    // so they scale with the user's font size.
    readonly property int topBarHeight: Kirigami.Units.gridUnit * 4
    // Thick seek bar (Plex-style chunky track with internal time
    // labels). The `TransportBar` host adds its own vertical
    // padding around this height.
    readonly property int seekBarHeightThick:
        Math.round(Kirigami.Units.gridUnit * 2.0)
    readonly property int iconButton:
        Kirigami.Units.gridUnit * 2 + Kirigami.Units.smallSpacing
    readonly property int iconButtonLg:    Kirigami.Units.gridUnit * 3

    // Vertical, always-visible volume widget. Width matches the
    // seek-bar thickness on purpose — the same idiom rotated 90°
    // (white played fill, dim rest, dotted handle, internal label).
    readonly property int volumeBarWidth:   seekBarHeightThick
    readonly property int volumeBarHeight:  Kirigami.Units.gridUnit * 12

    // Volume widget scale. The bar drives mpv's `volume` directly,
    // and mpv's `volume-max` is set to match in `MpvVideoItem`.
    // Boost above 100 % is intentional — useful for low-loudness
    // sources (poorly mastered music, quiet dialogue tracks).
    readonly property real volumeMaxPercent: 150

    // Shared stroke width for PathSvg-based icon glyphs in
    // IconGlyph.qml. Centralised so a future "thicker icons" pass
    // is one edit. Set to 2.0 to match Lucide's native stroke and
    // land on integer pixel boundaries at the default 24 px glyph
    // size — fractional widths straddle pixels and look soft.
    readonly property real iconStroke:     2.0

    // ---- Drop shadow tokens (PopupPanel, MultiEffect) ------------
    readonly property int   popupShadowSize:   Kirigami.Units.gridUnit * 2
    readonly property int   popupShadowOffset: Kirigami.Units.smallSpacing
    readonly property color popupShadowColor:  Qt.rgba(0.0, 0.0, 0.0, 0.55)

    // ---- Typography ----------------------------------------------
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
    // `tabularFont`). For one-off combos the raw `fontPoint*` values
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

    // Tabular (fixed-width) numerals in the user's proportional font,
    // for timecodes / durations / volume percent — anywhere digits
    // change in place and would otherwise jitter the layout because
    // proportional fonts give each digit a different advance width.
    //
    // `tnum` is the OpenType "tabular figures" feature, supported by
    // every modern UI font Plasma ships (Noto Sans, Cantarell, Inter,
    // Open Sans, IBM Plex …). `lnum` forces lining (cap-height)
    // figures, in case the font defaults to old-style figures for the
    // body weight. This matches what Plex / Netflix / YouTube do for
    // their on-screen timecodes — proportional sans, tabular digits,
    // not monospace.
    //
    // Requires Qt 6.6+ (font.features). The project's QT_MIN_VERSION
    // is pinned accordingly.
    readonly property var _tabularFeatures: ({ "tnum": 1, "lnum": 1 })

    readonly property font tabularFont: ({
        family:    Qt.application.font.family,
        pointSize: Qt.application.font.pointSize,
        features:  _tabularFeatures
    })
    readonly property font tabularSmallFont: ({
        family:    Qt.application.font.family,
        pointSize: Math.max(7, Qt.application.font.pointSize - 1),
        features:  _tabularFeatures
    })
}
