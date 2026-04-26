// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Series detail page: backdrop hero (no Play button), season tabs,
// vertical episode list, then an in-place expanding streams region
// for the selected episode, then the similar carousel. Pushed by
// `ApplicationShell.qml` on `MainController.showSeriesDetailRequested`.
//
// Esc pops via the shell-level shortcut. The page's own logic for
// "back from streams to episodes" maps to clearing the selected
// episode (the streams region collapses; episode list stays visible).
Kirigami.ScrollablePage {
    id: page

    objectName: "seriesDetail"
    title: seriesDetailVm.title.length > 0
        ? seriesDetailVm.title
        : i18nc("@title:window detail loading", "Loading\u2026")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    actions: [
        Kirigami.Action {
            icon.name: "view-sort-ascending"
            text: i18nc("@action:button", "Sort streams")
            displayHint: Kirigami.DisplayHint.IconOnly
                | Kirigami.DisplayHint.KeepVisible
            enabled: seriesDetailVm.streams
                && seriesDetailVm.streams.count > 0
            onTriggered: sortMenu.popup()
        },
        Kirigami.Action {
            icon.name: "subtitles"
            text: i18nc("@action:button", "Subtitles\u2026")
            enabled: seriesDetailVm.selectedEpisodeRow >= 0
            onTriggered: seriesDetailVm.requestSubtitles()
        }
    ]

    StreamSortMenu {
        id: sortMenu
        vm: seriesDetailVm
    }

    Component.onDestruction: seriesDetailVm.clear()

    ColumnLayout {
        width: page.width
        spacing: Kirigami.Units.largeSpacing * 2

        BackdropHero {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.heroHeight
            posterUrl:        seriesDetailVm.posterUrl
            backdropUrl:      seriesDetailVm.backdropUrl
            title:            seriesDetailVm.title
            year:             seriesDetailVm.year
            genres:           seriesDetailVm.genres
            runtimeMinutes:   0
            rating:           seriesDetailVm.rating
            description:      seriesDetailVm.description
            isUpcoming:       false
            releaseDateText:  seriesDetailVm.releaseDateText
            // No primary "Play best" on series \u2014 episode-level
            // selection drives playback. Only Subtitles surfaces.
            secondaryAction: Kirigami.Action {
                icon.name: "subtitles"
                text: i18nc("@action:button", "Subtitles\u2026")
                enabled: seriesDetailVm.selectedEpisodeRow >= 0
                onTriggered: seriesDetailVm.requestSubtitles()
            }
        }

        // Meta-error placeholder.
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignHCenter
            visible: seriesDetailVm.metaState
                === SeriesDetailViewModel.Error
            icon.name: "dialog-error"
            text: i18nc("@info placeholder",
                "Couldn't load this series.")
            explanation: seriesDetailVm.metaError
            helpfulAction: Kirigami.Action {
                icon.name: "view-refresh"
                text: i18nc("@action:button", "Retry")
                onTriggered: seriesDetailVm.retry()
            }
        }

        // Season picker + episode list + expanding streams.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing
            visible: seriesDetailVm.metaState
                === SeriesDetailViewModel.Ready

            SeasonTabs {
                Layout.fillWidth: true
                vm: seriesDetailVm
                visible: seriesDetailVm.seasonLabels.length > 0
            }

            // Episode list. The list is non-virtual so all rows in a
            // season are reachable by scroll \u2014 most shows have ~10
            // to ~25 episodes per season; rare outliers (Doctor Who,
            // SNL) get a tall list which the scroll page handles.
            ListView {
                id: episodeList
                Layout.fillWidth: true
                Layout.preferredHeight: contentHeight
                interactive: false
                model: seriesDetailVm.episodes
                spacing: 0
                cacheBuffer: Kirigami.Units.gridUnit * 40

                delegate: EpisodeRow {
                    episodeNumber: model.number
                    episodeTitle: model.title
                    description: model.description
                    releasedText: model.releasedText
                    isUpcoming: model.isUpcoming
                    thumbnailUrl: model.thumbnailUrl
                    selected: index
                        === seriesDetailVm.selectedEpisodeRow
                    onClicked: {
                        if (selected) {
                            seriesDetailVm.clearEpisode();
                        } else {
                            seriesDetailVm.selectEpisode(index);
                        }
                    }
                }
            }

            // Inline streams region \u2014 expands under the episode list
            // when an episode is selected. Capped to ~55% of viewport
            // so the user can still see the episode they came from.
            Kirigami.Heading {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                visible: seriesDetailVm.selectedEpisodeRow >= 0
                level: 4
                text: seriesDetailVm.selectedEpisodeLabel
                color: Theme.foreground
                wrapMode: Text.Wrap
            }
            StreamsList {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(
                    Kirigami.Units.gridUnit * 18,
                    page.height * 0.45)
                visible: seriesDetailVm.selectedEpisodeRow >= 0
                vm: seriesDetailVm
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
            Layout.preferredHeight: Kirigami.Units.largeSpacing * 2
        }
    }

    Kirigami.LoadingPlaceholder {
        anchors.centerIn: parent
        visible: seriesDetailVm.metaState
            === SeriesDetailViewModel.Loading
    }
}
