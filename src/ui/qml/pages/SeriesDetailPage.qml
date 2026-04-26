// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Full-width series detail surface. The overview pane owns series metadata and
// episode picking; the work pane shows streams/subtitles for the selected episode.
Kirigami.Page {
    id: page

    objectName: "seriesDetail"
    title: seriesDetailVm.title.length > 0
        ? seriesDetailVm.title
        : i18nc("@title:window detail loading", "Loading…")

    padding: 0

    actions: [
        Kirigami.Action {
            icon.name: "subtitles"
            text: i18nc("@action:button", "Subtitles…")
            displayHint: Kirigami.DisplayHint.IconOnly
                | Kirigami.DisplayHint.KeepVisible
            enabled: seriesDetailVm.selectedEpisodeRow >= 0
            onTriggered: seriesDetailVm.requestSubtitles()
        }
    ]

    function openSubtitlesPanel() {
        if (scaffold.workItem && scaffold.workItem.showSubtitles) {
            scaffold.workItem.showSubtitles();
        }
    }

    Component.onDestruction: seriesDetailVm.clear()

    DetailScaffold {
        id: scaffold
        anchors.fill: parent
        visible: seriesDetailVm.metaState === SeriesDetailViewModel.Ready

        overviewComponent: Component {
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing * 2

                DetailOverview {
                    Layout.fillWidth: true
                    posterUrl: seriesDetailVm.posterUrl
                    backdropUrl: seriesDetailVm.backdropUrl
                    title: seriesDetailVm.title
                    year: seriesDetailVm.year
                    genres: seriesDetailVm.genres
                    cast: seriesDetailVm.cast
                    runtimeMinutes: 0
                    rating: seriesDetailVm.rating
                    description: seriesDetailVm.description
                    isUpcoming: false
                    releaseDateText: seriesDetailVm.releaseDateText

                    secondaryAction: Kirigami.Action {
                        icon.name: "subtitles"
                        text: i18nc("@action:button", "Subtitles…")
                        enabled: seriesDetailVm.selectedEpisodeRow >= 0
                        onTriggered: seriesDetailVm.requestSubtitles()
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing

                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 3
                        text: i18nc("@title:section", "Episodes")
                    }

                    SeasonTabs {
                        Layout.fillWidth: true
                        vm: seriesDetailVm
                        visible: seriesDetailVm.seasonLabels.length > 0
                    }

                    ListView {
                        id: episodeList
                        Layout.fillWidth: true
                        Layout.preferredHeight: contentHeight
                        interactive: false
                        model: seriesDetailVm.episodes
                        spacing: Kirigami.Units.smallSpacing
                        cacheBuffer: Kirigami.Units.gridUnit * 40

                        delegate: EpisodeRow {
                            episodeNumber: model.number
                            episodeTitle: model.title
                            description: model.description
                            releasedText: model.releasedText
                            isUpcoming: model.isUpcoming
                            thumbnailUrl: model.thumbnailUrl
                            selected: index === seriesDetailVm.selectedEpisodeRow
                            onClicked: {
                                if (selected) {
                                    seriesDetailVm.clearEpisode();
                                } else {
                                    seriesDetailVm.selectEpisode(index);
                                    if (scaffold.workItem
                                        && scaffold.workItem.showStreams) {
                                        scaffold.workItem.showStreams();
                                    }
                                }
                            }
                        }
                    }
                }

                SimilarCarousel {
                    Layout.fillWidth: true
                    visible: seriesDetailVm.similarVisible
                    sourceModel: seriesDetailVm.similar
                    onItemActivated: function (row) {
                        seriesDetailVm.activateSimilar(row);
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
                detailVm: seriesDetailVm
                subtitleVm: subtitlesVm
                subtitlesAvailable: seriesDetailVm.selectedEpisodeRow >= 0
                subtitlesUnavailableText: i18nc("@info", "Pick an episode first.")
                onSubtitlesTabRequested: seriesDetailVm.requestSubtitles()
            }
        }
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4,
            Kirigami.Units.gridUnit * 30)
        visible: seriesDetailVm.metaState === SeriesDetailViewModel.Error
        icon.name: "dialog-error"
        text: i18nc("@info placeholder", "Couldn't load this series.")
        explanation: seriesDetailVm.metaError
        helpfulAction: Kirigami.Action {
            icon.name: "view-refresh"
            text: i18nc("@action:button", "Retry")
            onTriggered: seriesDetailVm.retry()
        }
    }

    Kirigami.LoadingPlaceholder {
        anchors.centerIn: parent
        visible: seriesDetailVm.metaState === SeriesDetailViewModel.Loading
    }
}
