// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Single-column movie detail surface. Streams have been split out into a
// dedicated `StreamsPage` reached via the `Play` action; subtitles use the
// pushed `SubtitlesPage` flow that `MainController` already provides. This
// page is intentionally focused on "what is this title?" — not "how do I
// watch it right now?".
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

    // Action set is intentionally minimal. `Streams` is the only verb
    // the detail page exposes — it pushes the streams page, where
    // subtitles becomes a header action against a chosen stream
    // context. Surfacing subtitles on the detail page added a second
    // verb without a useful target (no stream picked yet). The label
    // says "Streams" rather than "Play" because the action opens a
    // picker, not the player itself.
    readonly property Kirigami.Action streamsAction: Kirigami.Action {
        icon.name: "media-playback-start"
        text: i18nc("@action:button open the streams page",
            "Streams")
        displayHint: Kirigami.DisplayHint.IconOnly
            | Kirigami.DisplayHint.KeepVisible
        enabled: movieDetailVm.metaState === MovieDetailViewModel.Ready
        onTriggered: movieDetailVm.requestStreams()
    }

    actions: [ streamsAction ]

    Component.onDestruction: movieDetailVm.clear()

    // ---- Ready -----------------------------------------------------
    ColumnLayout {
        id: stack
        width: page.width
        spacing: Kirigami.Units.largeSpacing * 2
        visible: movieDetailVm.metaState === MovieDetailViewModel.Ready

        DetailOverview {
            Layout.fillWidth: true
            posterUrl: movieDetailVm.posterUrl
            backdropUrl: movieDetailVm.backdropUrl
            title: movieDetailVm.title
            year: movieDetailVm.year
            genres: movieDetailVm.genres
            cast: movieDetailVm.cast
            runtimeMinutes: movieDetailVm.runtimeMinutes
            rating: movieDetailVm.rating
            description: movieDetailVm.description
            isUpcoming: movieDetailVm.isUpcoming
            releaseDateText: movieDetailVm.releaseDateText

            primaryAction: page.streamsAction
        }

        SimilarCarousel {
            Layout.fillWidth: true
            visible: movieDetailVm.similarVisible
            sourceModel: movieDetailVm.similar
            onItemActivated: function (row) {
                movieDetailVm.activateSimilar(row);
            }
        }

        // Bottom breathing room so the last rail clears the scroll edge.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit
        }
    }

    // ---- Error -----------------------------------------------------
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4,
            Kirigami.Units.gridUnit * 30)
        visible: movieDetailVm.metaState === MovieDetailViewModel.Error
        icon.name: "dialog-error"
        text: i18nc("@info placeholder", "Couldn't load this title.")
        explanation: movieDetailVm.metaError
        helpfulAction: Kirigami.Action {
            icon.name: "view-refresh"
            text: i18nc("@action:button", "Retry")
            onTriggered: movieDetailVm.retry()
        }
    }

    // ---- Loading ---------------------------------------------------
    Kirigami.LoadingPlaceholder {
        anchors.centerIn: parent
        visible: movieDetailVm.metaState === MovieDetailViewModel.Loading
    }
}
