// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Library grid tile. Visual sibling of `PosterCard` — both build
// on `BasePosterCard` so the artwork frame, hover lift, focus
// ring, two-line wrapped title, and keyboard activation stay in
// lockstep across the app.
//
// What this wrapper adds on top of the base:
//
//   * a top-left chip with precedence `Watched → Upcoming → Year`
//     (mutually exclusive — only one of the two chips ever renders
//     in that corner),
//   * a top-right `RatingChip`,
//   * a per-row subtitle on the third meta line ("Resume from 42%",
//     "Releases Apr 26", "3 / 8 episodes watched"),
//   * the progress bar binding for in-progress entries (driven by
//     `KinemaArtworkFrame`'s built-in overlay via the base's
//     `progress` prop),
//   * a right-click context menu (Open / toggle Watched / Remove
//     from Library), wired through the base's `rightClicked()`
//     signal.
BasePosterCard {
    id: card

    // ---- Inputs --------------------------------------------------
    property int year: 0
    property real rating: -1
    property bool watched: false
    property bool upcoming: false

    // `posterUrl`, `title`, `subtitle`, `progress`, `clicked()`
    // come from the base.

    signal removeRequested()
    signal toggleWatchedRequested()

    onRightClicked: contextMenu.popup()

    // Top-left precedence: Watched → Upcoming → Year. `StatusChip`
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

    QQC2.Menu {
        id: contextMenu

        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Open")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: card.clicked()
        }
        QQC2.MenuItem {
            text: card.watched
                ? i18nc("@action:inmenu", "Mark as Unwatched")
                : i18nc("@action:inmenu", "Mark as Watched")
            icon.source: AppIcons.url(card.watched
                ? "circle-dashed" : "circle-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !card.upcoming
            onTriggered: card.toggleWatchedRequested()
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Remove from Library\u2026")
            icon.source: AppIcons.url("library")
            icon.color: AppIcons.negative
            onTriggered: card.removeRequested()
        }
    }
}
