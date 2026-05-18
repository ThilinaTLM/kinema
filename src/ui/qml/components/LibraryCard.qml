// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Library grid tile. Visual sibling of `PosterCard` \u2014 both build
// on `BasePosterCard` so the artwork frame, hover lift, focus
// ring, two-line wrapped title, and keyboard activation stay in
// lockstep across the app.
//
// What this wrapper adds on top of the base:
//
//   * a top-left chip with precedence `Watched \u2192 Upcoming \u2192 Year`
//     (mutually exclusive \u2014 only one of the two chips ever renders
//     in that corner),
//   * a top-right `RatingChip`,
//   * a per-row subtitle on the third meta line ("Resume from 42%",
//     "Releases Apr 26", "3 / 8 episodes watched"),
//   * the progress bar binding for in-progress entries (driven by
//     `KinemaArtworkFrame`'s built-in overlay via the base's
//     `progress` prop),
//   * a right-click context menu that matches the conventions in
//     `docs/MenuConventions.md`: Details / Find Streams / Mark
//     Watched / Copy Title / Open on IMDb / Remove from
//     Library\u2026 \u2014 wired through the base's `rightClicked()`
//     signal.
BasePosterCard {
    id: card

    // ---- Inputs --------------------------------------------------
    property int year: 0
    property real rating: -1
    property bool watched: false
    property bool upcoming: false
    /// IMDb id of the saved title. Drives the menu's
    /// "Open on IMDb" item and the "Find Streams" route.
    property string imdbId: ""
    /// `domain::MediaKind` value (0 = Movie, 1 = Series). Disables
    /// "Mark Watched" on series rows (toggle is per-episode on the
    /// detail page).
    property int kindIndex: 0

    // `posterUrl`, `title`, `subtitle`, `progress`, `clicked()`
    // come from the base.

    signal removeRequested()
    signal toggleWatchedRequested()
    /// Per-row context menu signals. The consumer delegate
    /// (`LibraryPage.qml`) wires these to the matching VM slots.
    signal findStreamsRequested()

    onRightClicked: contextMenu.popup()

    // Top-left precedence: Watched \u2192 Upcoming \u2192 Year. `StatusChip`
    // self-hides when neither flag is set; `YearChip` is explicitly
    // suppressed while a status is showing so the two never
    // overlap.
    YearChip {
        year: card.year
        visible: card.year > 0 && !card.watched && !card.upcoming
        anchors {
            top: parent.top
            left: parent.left
            topMargin: Kirigami.Units.smallSpacing
            leftMargin: Kirigami.Units.smallSpacing
        }
    }

    StatusChip {
        watched: card.watched
        upcoming: card.upcoming
        anchors {
            top: parent.top
            left: parent.left
            topMargin: Kirigami.Units.smallSpacing
            leftMargin: Kirigami.Units.smallSpacing
        }
    }

    RatingChip {
        rating: card.rating
        anchors {
            top: parent.top
            right: parent.right
            topMargin: Kirigami.Units.smallSpacing
            rightMargin: Kirigami.Units.smallSpacing
        }
    }

    KinemaMenu {
        id: contextMenu

        KinemaMenuItem {
            iconName: "info"
            label: i18nc("@action:inmenu library card", "Details")
            onTriggered: card.clicked()
        }
        KinemaMenuItem {
            iconName: "list-video"
            label: i18nc("@action:inmenu library card", "Streams")
            // Upcoming rows have no streams yet; the indexers
            // return nothing useful for unaired episodes / titles.
            enabled: !card.upcoming
            onTriggered: card.findStreamsRequested()
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: card.watched ? "circle-dashed" : "circle-check"
            label: card.watched
                ? i18nc("@action:inmenu library card", "Mark Unwatched")
                : i18nc("@action:inmenu library card", "Mark Watched")
            // Disabled for upcoming rows (nothing to mark watched
            // yet) and for series rows (per-episode mutation lives
            // on the detail page; using a single click here would
            // silently mark the whole series watched, which is
            // rarely what the user wants).
            enabled: !card.upcoming && card.kindIndex !== 1
            onTriggered: card.toggleWatchedRequested()
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: "copy"
            label: i18nc("@action:inmenu library card", "Copy Title")
            enabled: card.title.length > 0
            onTriggered: shell.copyToClipboard(card.title,
                i18nc("@info:status", "Title copied to clipboard"))
        }
        KinemaMenuItem {
            iconName: "external-link"
            label: i18nc("@action:inmenu library card", "Open on IMDb")
            enabled: card.imdbId.length > 0
            onTriggered: shell.openImdbTitle(card.imdbId)
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: "trash-2"
            label: i18nc("@action:inmenu library card", "Remove")
            destructive: true
            onTriggered: card.removeRequested()
        }
    }
}
