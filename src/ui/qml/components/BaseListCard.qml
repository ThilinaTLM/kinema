// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Shared full-row list-card chassis. Mirrors `BasePosterCard.qml` for
// rows: owns the background, hover overlay, selection styling, focus
// ring, padding, list-margin-aware width, right-click semantics, an
// optional hover-revealed "navigate forward" chevron, and an optional
// row-level progress bar.
//
// Used by `EpisodeListCard`, `StreamListCard`, `DownloadListCard`,
// `SubtitleListCard`.
//
// The card body is composed via three `data` aliases over layout
// hosts so children declared inline pick up the host layout
// naturally:
//
//   * `leading`  — RowLayout host. Typically a single thumbnail /
//                  poster Item using `Layout.preferredWidth/Height`.
//   * `body`     — ColumnLayout host (`default property`). Children
//                  stack vertically; declare a RowLayout child for a
//                  horizontal body shape (e.g. StreamListCard).
//   * `trailing` — RowLayout host for action buttons / overflow.
//
// The chassis exposes `slotAlignment` so cards with a tall body
// (DownloadListCard) can pin leading + trailing to the top.
QQC2.ItemDelegate {
    id: card

    // ---- Inputs --------------------------------------------------
    /// Visual selection state. When true the chassis paints an
    /// accent-tinted background + accent border (matches the
    /// EpisodeRow precedent that becomes the app-wide canonical).
    property bool selected: false

    /// Optional row-level progress bar drawn below the body row.
    /// `< 0` or `>= 1` hides it. Style is the translucent track +
    /// theme highlight fill that EpisodeRow / DownloadRow both
    /// hand-rolled.
    property real progress: -1

    /// Optional "this row navigates" affordance. When non-empty the
    /// chassis fades in a small chevron-right at the trailing edge
    /// on `hovered || activeFocus`, and surfaces this string as the
    /// hover tooltip on the row. Empty by default (chevron hidden).
    property string navigationHint: ""

    /// Tooltip surfaced when the row's title has elided. Cards bind
    /// this from their title `Label.truncated` so long names stay
    /// reachable on hover without a per-row HoverHandler.
    property string titleTooltip: ""

    /// Vertical alignment applied uniformly to the three slots in
    /// the body row. Default `Qt.AlignVCenter`; DownloadListCard
    /// passes `Qt.AlignTop` so the poster + action rail pin to the
    /// row's top edge alongside a multi-line body.
    property int slotAlignment: Qt.AlignVCenter

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
    padding: Theme.pageMargin
    implicitHeight: contentColumn.implicitHeight + padding * 2

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

    contentItem: ColumnLayout {
        id: contentColumn
        spacing: Theme.inlineSpacing

        RowLayout {
            id: bodyRow
            Layout.fillWidth: true
            spacing: Theme.groupSpacing

            // Leading slot — a RowLayout so a single thumbnail child
            // sizes via Layout.preferredWidth/Height. `visible`
            // collapses the slot when no leading item is set.
            RowLayout {
                id: leadingHost
                Layout.alignment: card.slotAlignment
                visible: leadingHost.children.length > 0
                spacing: 0
            }

            // Body slot — ColumnLayout so children declared inline
            // stack vertically. Cards needing a horizontal body
            // (StreamListCard's three-column layout) declare a
            // single RowLayout child.
            ColumnLayout {
                id: bodyHost
                Layout.fillWidth: true
                Layout.alignment: card.slotAlignment
                spacing: Theme.inlineSpacing
            }

            // Trailing slot — RowLayout for action buttons.
            RowLayout {
                id: trailingHost
                Layout.alignment: card.slotAlignment
                visible: trailingHost.children.length > 0
                spacing: Theme.inlineSpacing
            }

            // Hover-revealed forward-navigation chevron. Sits at the
            // far trailing edge; visibility driven by
            // `navigationHint` so cards that don't navigate omit it.
            Item {
                Layout.preferredWidth: chevron.visible
                    ? chevron.implicitWidth + Kirigami.Units.smallSpacing
                    : 0
                Layout.preferredHeight: chevron.implicitHeight
                Layout.alignment: card.slotAlignment
                visible: card.navigationHint.length > 0

                Kirigami.Icon {
                    id: chevron
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: implicitWidth
                    source: AppIcons.url("chevron-right")
                    color: Theme.disabled
                    opacity: card.hovered || card.activeFocus ? 1 : 0
                    Behavior on opacity {
                        NumberAnimation {
                            duration: Kirigami.Units.shortDuration
                        }
                    }
                }
            }
        }

        // Row-level progress bar. Same translucent track + theme
        // highlight fill that EpisodeRow / DownloadRow both hand-
        // rolled.
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
