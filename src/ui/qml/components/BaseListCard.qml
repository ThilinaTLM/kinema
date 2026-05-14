// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Shared full-row list-card chassis. Mirrors `BasePosterCard.qml` for
// rows: owns the background, hover overlay, selection styling, focus
// ring, padding, list-margin-aware width, right-click semantics, and
// an optional row-level progress bar.
//
// Used by `EpisodeListCard`, `StreamListCard`, `DownloadListCard`,
// `SubtitleListCard`.
//
// Two-column shape:
//
//   ┌──────────┬─────────────────────────────────────┐
//   │          │ body          (content slot)        │
//   │ leading  │ ─── optional progress bar ───       │
//   │          │ trailing      (action row slot)     │
//   └──────────┴─────────────────────────────────────┘
//
// The card body exposes three `data` aliases over layout hosts so
// children declared inline pick up the host layout naturally:
//
//   * `leading`  — RowLayout host pinned to the card's top edge,
//                  typically a single thumbnail / poster /
//                  `RowLeadingTile` Item using
//                  `Layout.preferredWidth/Height`.
//   * `body`     — ColumnLayout host (`default property`). Children
//                  stack vertically; declare a RowLayout child for a
//                  horizontal body shape (e.g. StreamListCard).
//   * `trailing` — RowLayout host for action buttons / overflow.
//                  Rendered as a row *below* the body, not to the
//                  right of it. Collapses when empty.
//
// Chassis padding carries the app-wide page gutter. The card
// background reaches the page edge of the host `ListView` (lists
// using `ListSurface` default to zero horizontal margins, so the
// card chrome spans full width and hover / selection / focus
// tints fill the row edge to edge). The card's *content* is then
// inset by `Theme.pageMargin` left/right via this chassis
// padding, landing card content at the same horizontal offset as
// the detail-page hero, the Discover rails, the Library grid,
// and `PageHeaderBar` titles. Vertical padding (`Theme.groupSpacing`)
// stays tighter than horizontal so adjacent rows don't drift
// apart from internal padding alone — the leading thumbnail is
// therefore equidistant only from the left edge; its top/bottom
// inset is the smaller vertical padding. When the host list owns
// its own vertical scrollbar (`Kirigami.Page` flavors), the
// `ListSurface` reserves a dedicated right-side strip for the
// bar so the card chrome stops at the scrollbar's left edge —
// the card's own `rightPadding` (= `Theme.pageMargin`) then
// provides the canonical content gutter measured from the
// scrollbar inward, mirroring how `Kirigami.ScrollablePage`
// paints its overlay scrollbar in dedicated chrome outside the
// content area on detail pages.
QQC2.ItemDelegate {
    id: card

    // ---- Inputs --------------------------------------------------
    /// Visual selection state. When true the chassis paints an
    /// accent-tinted background + accent border (matches the
    /// EpisodeRow precedent that becomes the app-wide canonical).
    property bool selected: false

    /// Optional row-level progress bar drawn between the body and
    /// the action row, inside the right column.
    /// `<= 0` or `>= 1` hides it. Style is the translucent track +
    /// theme highlight fill that EpisodeRow / DownloadRow both
    /// hand-rolled.
    property real progress: -1

    /// Optional "this row navigates" affordance. When non-empty the
    /// chassis surfaces this string as the hover tooltip on the row
    /// (takes precedence over `titleTooltip`). Empty by default.
    property string navigationHint: ""

    /// Tooltip surfaced when the row's title has elided. Cards bind
    /// this from their title `Label.truncated` so long names stay
    /// reachable on hover without a per-row HoverHandler.
    property string titleTooltip: ""

    // ---- Slots ---------------------------------------------------
    default property alias body: bodyHost.data
    property alias leading: leadingHost.data
    property alias trailing: trailingHost.data

    // ---- Outputs -------------------------------------------------
    /// Double-click / Return / Enter. `onClicked` (from
    /// `ItemDelegate`) carries single-click activation separately.
    signal activated()

    /// Right-click or context-menu key. The point is in card-local
    /// coordinates so callers can pass it straight to
    /// `Menu.popup(x, y)`.
    signal contextMenuRequested(point pt)

    // ---- Sizing / chrome -----------------------------------------
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    hoverEnabled: true
    activeFocusOnTab: true
    focusPolicy: Qt.StrongFocus

    width: ListView.view
        ? ListView.view.width - ListView.view.leftMargin
            - ListView.view.rightMargin
        : implicitWidth
    leftPadding:   Theme.pageMargin
    rightPadding:  Theme.pageMargin
    topPadding:    Theme.groupSpacing
    bottomPadding: Theme.groupSpacing

    onDoubleClicked: card.activated()
    Keys.onReturnPressed: card.activated()
    Keys.onEnterPressed: card.activated()
    Keys.onMenuPressed: card.contextMenuRequested(
        Qt.point(card.width / 2, card.height / 2))

    // Right-click anywhere on the row. `gesturePolicy:
    // ReleaseWithinBounds` so a press-and-drag off the row cancels
    // the menu, matching QQC2.Menu's own grabber.
    TapHandler {
        acceptedButtons: Qt.RightButton
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: function (eventPoint) {
            card.contextMenuRequested(eventPoint.position);
        }
    }

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.alternateBackgroundColor
        border.color: card.selected
            ? Qt.alpha(Theme.accent, 0.36)
            : (card.activeFocus
                ? Kirigami.Theme.focusColor
                : Qt.alpha(Theme.foreground,
                    card.hovered ? 0.14 : 0.08))
        border.width: card.activeFocus && !card.selected ? 2 : 1

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: card.selected
                ? Qt.alpha(Theme.accent, 0.18)
                : (card.hovered
                    ? Qt.alpha(Theme.hover, 0.12)
                    : "transparent")
        }
    }

    contentItem: RowLayout {
        id: bodyRow
        spacing: Theme.groupSpacing

        // Leading slot — a RowLayout so a single thumbnail child
        // sizes via `Layout.preferredWidth` (the child's actual
        // height drives its width via aspect). The host pins its
        // own `Layout.preferredHeight` to `rightColumn`'s outer box
        // (`implicitHeight` plus its top/bottom `smallSpacing`
        // margins) so the row's height is driven entirely by the
        // right column; the child thumbnail's `Layout.fillHeight:
        // true` then stretches it to exactly match — meaning the
        // thumbnail spans `rightColumn`'s margin box and is
        // ~`2 * smallSpacing` taller than the column's content area.
        // Without this binding, the child thumbnail's
        // `RowMediaThumbnail.implicitHeight` (`Theme.posterMin * 1.5`)
        // would propagate up and make `bodyRow` taller than the
        // right column's content, leaving slack below the trailing
        // action row. `visible` collapses the slot when no leading
        // item is set.
        RowLayout {
            id: leadingHost
            Layout.fillHeight: true
            Layout.preferredHeight: rightColumn.implicitHeight
                + 2 * Kirigami.Units.smallSpacing
            visible: leadingHost.children.length > 0
            spacing: 0
        }

        // Right column — content (body slot) above an optional
        // progress bar above the action row (trailing slot). A
        // uniform `smallSpacing` inset on top / right / bottom
        // gives body, progress bar, and trailing action row the
        // same right edge and the same vertical breathing room
        // against the chassis. No left inset — body content stays
        // flush against `bodyRow.spacing` from the leading column.
        // The column itself has no `fillHeight`, so it is sized to
        // its content; the leading thumbnail then stretches to
        // match `implicitHeight + 2 * smallSpacing` (see leading
        // slot above).
        ColumnLayout {
            id: rightColumn
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: Kirigami.Units.smallSpacing
            Layout.rightMargin: Kirigami.Units.smallSpacing
            Layout.bottomMargin: Kirigami.Units.smallSpacing
            spacing: Theme.inlineSpacing

            // Body slot — ColumnLayout so children declared inline
            // stack vertically. Cards needing a horizontal body
            // (StreamListCard's two-column layout) declare a
            // single RowLayout child.
            ColumnLayout {
                id: bodyHost
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing
            }

            // Row-level progress bar. Same translucent track +
            // theme highlight fill that EpisodeRow / DownloadRow
            // both hand-rolled. Sits between content and actions
            // so it reads as "this row is loading toward action-
            // ability".
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.smallSpacing
                visible: card.progress > 0 && card.progress < 1
                radius: height / 2
                color: Qt.alpha(Theme.foreground, 0.14)

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: parent.width
                        * Math.max(0, Math.min(1, card.progress))
                    radius: parent.radius
                    color: Theme.accent
                }
            }

            // Action row — RowLayout for action buttons. Rendered
            // below the body. Collapses when no actions are
            // declared.
            RowLayout {
                id: trailingHost
                Layout.fillWidth: true
                visible: trailingHost.children.length > 0
                spacing: Theme.inlineSpacing
            }
        }

    }

    // Truncation / navigation tooltip. Single QQC2.ToolTip surfaced
    // when a card sets `titleTooltip` or `navigationHint`.
    // Navigation hint takes precedence so the row's destination is
    // surfaced even when the title also elides.
    QQC2.ToolTip {
        visible: card.hovered
            && (card.titleTooltip.length > 0
                || card.navigationHint.length > 0)
        delay: Kirigami.Units.toolTipDelay
        text: card.navigationHint.length > 0
            ? card.navigationHint
            : card.titleTooltip
    }
}
