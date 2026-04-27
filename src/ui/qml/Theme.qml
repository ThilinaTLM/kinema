// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

pragma Singleton

import QtQuick
import org.kde.kirigami as Kirigami

// Thin Kirigami façade for the app shell. Sizes derive from
// `Kirigami.Units` so layouts scale with the user's KDE font / DPI
// settings; colours derive from `Kirigami.Theme` with the singleton
// pinned to the Window colorSet (`view` / window background palette)
// so the shell follows the user's Plasma colour scheme without any
// custom palette of our own.
//
// The player module owns its own `Theme.qml` pinned to the Header
// colorSet for chrome over video. Two themes, two responsibilities;
// no cross-imports.
QtObject {
    id: theme

    // Pin the singleton to the window palette. Children that resolve
    // `Theme.foreground` / `Theme.background` etc. inherit through
    // this object.
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    Kirigami.Theme.inherit: false

    // ---- Palette --------------------------------------------------
    readonly property color foreground:    Kirigami.Theme.textColor
    readonly property color background:    Kirigami.Theme.backgroundColor
    readonly property color accent:        Kirigami.Theme.highlightColor
    readonly property color accentText:    Kirigami.Theme.highlightedTextColor
    readonly property color positive:      Kirigami.Theme.positiveTextColor
    readonly property color negative:      Kirigami.Theme.negativeTextColor
    readonly property color hover:         Kirigami.Theme.hoverColor
    readonly property color disabled:      Kirigami.Theme.disabledTextColor

    // ---- Sizes / spacing -----------------------------------------
    readonly property int unit:        Kirigami.Units.smallSpacing
    readonly property int spacing:     Kirigami.Units.largeSpacing
    readonly property int gridUnit:    Kirigami.Units.gridUnit
    readonly property int radius:      Kirigami.Units.cornerRadius

    // ---- Layout rhythm -------------------------------------------
    // Page and component spacing tokens. They are still Kirigami.Unit-
    // derived, but centralising the names keeps page gutters and state
    // surfaces aligned across Discover, Search, Browse, details,
    // streams, subtitles, and settings.
    readonly property int pageMargin:        Kirigami.Units.gridUnit
    readonly property int pageWideMargin:    Kirigami.Units.gridUnit * 2
    readonly property int pageTopSpacing:    Kirigami.Units.smallSpacing
    readonly property int pageBottomSpacing: Kirigami.Units.gridUnit
    readonly property int inlineSpacing:     Kirigami.Units.smallSpacing
    readonly property int groupSpacing:      Kirigami.Units.largeSpacing
    readonly property int sectionSpacing:    Kirigami.Units.largeSpacing * 2

    readonly property int placeholderMaxWidth:
        Kirigami.Units.gridUnit * 28
    readonly property int detailPlaceholderMaxWidth:
        Kirigami.Units.gridUnit * 30
    readonly property int wideContentMaxWidth:
        Kirigami.Units.gridUnit * 36

    // Poster / hero structural sizes — gridUnit-derived so they
    // scale with the user's font size.
    readonly property int posterMin:   Kirigami.Units.gridUnit * 8
    readonly property int posterMax:   Kirigami.Units.gridUnit * 12
    readonly property int heroHeight:  Kirigami.Units.gridUnit * 18

    // ---- Typography ----------------------------------------------
    // Inherits the user's KDE "General font" via Qt.application.font,
    // which the KDE platform plugin populates from
    // ~/.config/kdeglobals [General].font.
    readonly property font defaultFont: Qt.application.font

    readonly property font titleFont: ({
        family:    Qt.application.font.family,
        pointSize: Qt.application.font.pointSize + 4,
        weight:    Font.DemiBold
    })

    readonly property font sectionFont: ({
        family:    Qt.application.font.family,
        pointSize: Qt.application.font.pointSize + 2,
        weight:    Font.DemiBold
    })

    readonly property font captionFont: ({
        family:    Qt.application.font.family,
        pointSize: Math.max(7, Qt.application.font.pointSize - 1)
    })
}
