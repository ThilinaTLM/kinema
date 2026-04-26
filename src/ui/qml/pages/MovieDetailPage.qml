// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Full-width movie detail surface. The application PageRow stays single-column;
// this page owns the responsive split between overview and streams/subtitles.
Kirigami.Page {
    id: page

    objectName: "movieDetail"
    title: movieDetailVm.title.length > 0
        ? movieDetailVm.title
        : i18nc("@title:window detail loading", "Loading…")

    padding: 0

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
            icon.name: "subtitles"
            text: i18nc("@action:button", "Subtitles…")
            enabled: movieDetailVm.metaState === MovieDetailViewModel.Ready
            onTriggered: movieDetailVm.requestSubtitles()
        }
    ]

    function openSubtitlesPanel() {
        if (scaffold.workItem && scaffold.workItem.showSubtitles) {
            scaffold.workItem.showSubtitles();
        }
    }

    Component.onDestruction: movieDetailVm.clear()

    DetailScaffold {
        id: scaffold
        anchors.fill: parent
        visible: movieDetailVm.metaState === MovieDetailViewModel.Ready

        overviewComponent: Component {
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing * 2

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

                    primaryAction: Kirigami.Action {
                        icon.name: "media-playback-start"
                        text: i18nc("@action:button", "Play best")
                        enabled: movieDetailVm.streams
                            && movieDetailVm.streams.count > 0
                        onTriggered: movieDetailVm.playBest()
                    }
                    secondaryAction: Kirigami.Action {
                        icon.name: "subtitles"
                        text: i18nc("@action:button", "Subtitles…")
                        enabled: movieDetailVm.metaState
                            === MovieDetailViewModel.Ready
                        onTriggered: movieDetailVm.requestSubtitles()
                    }
                }

                SimilarCarousel {
                    Layout.fillWidth: true
                    visible: movieDetailVm.similarVisible
                    sourceModel: movieDetailVm.similar
                    onItemActivated: function (row) {
                        movieDetailVm.activateSimilar(row);
                    }
                }

                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.largeSpacing
                }
            }
        }

        workComponent: Component {
            DetailWorkPanel {
                detailVm: movieDetailVm
                subtitleVm: subtitlesVm
                subtitlesAvailable: movieDetailVm.metaState
                    === MovieDetailViewModel.Ready
                subtitlesUnavailableText: i18nc("@info", "Movie details are still loading.")
                onSubtitlesTabRequested: movieDetailVm.requestSubtitles()
            }
        }
    }

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

    Kirigami.LoadingPlaceholder {
        anchors.centerIn: parent
        visible: movieDetailVm.metaState === MovieDetailViewModel.Loading
    }
}
