// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Shared poster-card chassis used by `PosterCard` (Discover /
// Browse / Search / Similar) and `LibraryCard` (Library grid).
// Owns everything the two cards used to duplicate:
//
//   * the `KinemaArtworkFrame` (2:3 by default, configurable via
//     `aspect`), hover/focus state, and the optional progress bar
//     overlay along the bottom inset,
//   * a fixed two-line wrapped title heading so short titles keep
//     grid rows aligned and long titles wrap to a second line
//     instead of eliding mid-word,
//   * an optional single-line subtitle below the title (hidden
//     when empty),
//   * keyboard activation (Return / Enter / Space) and left- /
//     right-click activation via `clicked()` and `rightClicked()`
//     signals.
//
// Chips and badges are layered on top of the artwork via the
// `default property alias overlays` slot, which forwards into the
// frame's own overlay slot. Subclasses just drop their chips
// directly into the card body and anchor them against `parent` —
// `parent` resolves to the frame, exactly the way both cards used
// to anchor their chips before the refactor.
//
// `EpisodeRailCard` intentionally stays separate: different aspect,
// thumbnail → backdrop → poster fallback chain, three meta lines,
// no chips. Folding it in would balloon this base for no win.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
    property string posterUrl
    property string title
    /// Optional caption line under the title. Hidden when empty so
    /// `PosterCard` (no subtitle) and `LibraryCard` (always has one)
    /// share the same meta block without conditional visibility at
    /// the call site.
    property string subtitle: ""
    /// `< 0` or `>= 1` hides the bar entirely (matches the frame's
    /// own progress contract).
    property real progress: -1
    /// Height / width. 1.5 = 2:3 poster, 9/16 = 16:9 still. Kept
    /// configurable for forward-compat but every current consumer
    /// stays on the 2:3 default.
    property real aspect: 1.5
    property string fallbackIcon: "film"

    /// Chip / badge slot. Children become overlays inside the
    /// artwork frame; anchor them against `parent` and they sit
    /// on top of the image inside the rounded clip.
    default property alias overlays: poster.overlays

    // ---- Outputs -------------------------------------------------
    signal clicked()
    signal rightClicked()

    // Single source of truth for hover so the frame's hover/focus
    // inputs and any future overlay tweaks stay in sync.
    readonly property bool _hovered: hoverHandler.hovered

    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    implicitWidth: Theme.posterMin
    implicitHeight: poster.height + meta.implicitHeight
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
            id: poster
            Layout.fillWidth: true
            Layout.preferredHeight: Math.round(width * card.aspect)

            url: card.posterUrl
            aspect: card.aspect
            fallbackIcon: card.fallbackIcon
            hovered: card._hovered
            focusRing: card._hovered || card.activeFocus
            progress: card.progress
        }

        ColumnLayout {
            id: meta
            Layout.fillWidth: true
            spacing: 0

            Kirigami.Heading {
                Layout.fillWidth: true
                // Natural height: 1 line for short titles, 2 lines
                // for wrapped titles. The grid's `cellHeight`
                // already budgets for the 2-line worst case, so
                // short-title cards just leave the slack at the
                // bottom — keeping the title and subtitle visually
                // glued instead of separated by an empty reserved
                // line.
                level: 5
                text: card.title
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                maximumLineCount: 2
                color: Kirigami.Theme.textColor
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: card.subtitle.length > 0
                text: card.subtitle
                elide: Text.ElideRight
                maximumLineCount: 1
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    // Click + hover handlers. TapHandler covers click + keyboard-
    // synthesised activation via `Keys.onPressed` above. Right-click
    // is opt-in: subclasses that want a context menu (LibraryCard)
    // connect to `rightClicked()`; others leave it unconnected and
    // right-click is a no-op.
    TapHandler {
        acceptedButtons: Qt.LeftButton
        onTapped: card.clicked()
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: card.rightClicked()
    }
    HoverHandler {
        id: hoverHandler
        cursorShape: Qt.PointingHandCursor
    }
}
