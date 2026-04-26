// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Movie detail page: backdrop hero on top, streams card list under
// it, similar carousel below. Pushed by `ApplicationShell.qml` on
// `MainController.showMovieDetailRequested`. The view-model has
// already been told to load by the time the page mounts; we just
// render against `movieDetailVm` and route action signals back into
// it.
//
// Esc pops via the shell-level shortcut (`pageStack.pop()` when
// `depth > 1`). The page itself owns no extra Esc handling.
Kirigami.ScrollablePage {
    id: page

    objectName: "movieDetail"
    title: movieDetailVm.title.length > 0
        ? movieDetailVm.title
        : i18nc("@title:window detail loading", "Loading\u2026")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    // Header actions. "Sort streams" exposes the menu through
    // `Kirigami.Action.children`; the menu items are built lazily
    // inside `StreamSortMenu.qml`. The Subtitles action is wired
    // to a stub passive notification until phase 06.
    actions: [
        Kirigami.Action {
            icon.name: "media-playback-start"
            text: i18nc("@action:button", "Play best")
            displayHint: Kirigami.DisplayHint.IconOnly
                | Kirigami.DisplayHint.KeepVisible
            enabled: movieDetailVm.streams
                && movieDetailVm.streams.count > 0
            onTriggered: movieDetailVm.playBest()
        },
        Kirigami.Action {
            icon.name: "view-sort-ascending"
            text: i18nc("@action:button", "Sort streams")
            enabled: movieDetailVm.streams
                && movieDetailVm.streams.count > 0
            onTriggered: sortMenu.popup()
        },
        Kirigami.Action {
            icon.name: "subtitles"
            text: i18nc("@action:button", "Subtitles\u2026")
            onTriggered: movieDetailVm.requestSubtitles()
        }
    ]

    StreamSortMenu {
        id: sortMenu
        vm: movieDetailVm
    }

    // Reset the VM when the page is popped so a future push lands
    // on a clean surface (no stale poster, streams, or similar
    // rail). `Component.onDestruction` is the right hook because
    // `pageStack.pop()` destroys the popped page.
    Component.onDestruction: movieDetailVm.clear()

    ColumnLayout {
        width: page.width
        spacing: Kirigami.Units.largeSpacing * 2

        BackdropHero {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.heroHeight
            posterUrl:        movieDetailVm.posterUrl
            backdropUrl:      movieDetailVm.backdropUrl
            title:            movieDetailVm.title
            year:             movieDetailVm.year
            genres:           movieDetailVm.genres
            runtimeMinutes:   movieDetailVm.runtimeMinutes
            rating:           movieDetailVm.rating
            description:      movieDetailVm.description
            isUpcoming:       movieDetailVm.isUpcoming
            releaseDateText:  movieDetailVm.releaseDateText

            primaryAction: Kirigami.Action {
                icon.name: "media-playback-start"
                text: i18nc("@action:button", "Play best")
                enabled: movieDetailVm.streams
                    && movieDetailVm.streams.count > 0
                onTriggered: movieDetailVm.playBest()
            }
            secondaryAction: Kirigami.Action {
                icon.name: "subtitles"
                text: i18nc("@action:button", "Subtitles\u2026")
                onTriggered: movieDetailVm.requestSubtitles()
            }
        }

        // Meta-error placeholder. Replaces the hero when meta failed
        // (so the user sees the retry affordance prominently).
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignHCenter
            visible: movieDetailVm.metaState
                === MovieDetailViewModel.Error
            icon.name: "dialog-error"
            text: i18nc("@info placeholder",
                "Couldn't load this title.")
            explanation: movieDetailVm.metaError
            helpfulAction: Kirigami.Action {
                icon.name: "view-refresh"
                text: i18nc("@action:button", "Retry")
                onTriggered: movieDetailVm.retry()
            }
        }

        StreamsList {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(
                Kirigami.Units.gridUnit * 24,
                page.height * 0.5)
            visible: movieDetailVm.metaState
                === MovieDetailViewModel.Ready
            vm: movieDetailVm
        }

        SimilarCarousel {
            Layout.fillWidth: true
            visible: movieDetailVm.similarVisible
            sourceModel: movieDetailVm.similar
            onItemActivated: function (row) {
                movieDetailVm.activateSimilar(row);
            }
        }

        // Bottom breathing room so the similar carousel doesn't kiss
        // the scroll edge.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.largeSpacing * 2
        }
    }

    // Centered loading indicator while meta resolves.
    Kirigami.LoadingPlaceholder {
        anchors.centerIn: parent
        visible: movieDetailVm.metaState
            === MovieDetailViewModel.Loading
    }
}
