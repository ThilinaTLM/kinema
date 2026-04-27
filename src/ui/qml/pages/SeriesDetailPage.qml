// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Single-column series detail surface. The episode list IS the play
// affordance: tapping an episode selects it AND pushes the streams page,
// matching the previous "auto-switch to Streams tab" UX from the work
// panel. Esc returns to this page with the selection preserved.
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

    // No page-level actions: the episode list IS the play affordance,
    // and subtitles only have a meaningful context once a stream is
    // chosen, so it lives on `StreamsPage` instead.

    Component.onDestruction: seriesDetailVm.clear()

    // ---- Ready -----------------------------------------------------
    ColumnLayout {
        id: stack
        width: page.width
        spacing: Theme.sectionSpacing
        visible: seriesDetailVm.metaState === SeriesDetailViewModel.Ready

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

            // No primary / secondary action: the episode list below
            // is the play affordance, and subtitles live on the
            // `StreamsPage` that an episode tap pushes.
        }

        // The episode list and season tabs already indent their
        // content via internal padding (`EpisodeRow.padding`,
        // `SeasonTabs` Row `leftPadding`), so this column only adds
        // page-edge margin to the heading. Letting the list span
        // edge-to-edge keeps episode content aligned with the title
        // above and lets row hover backgrounds reach the page edge.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.groupSpacing

            Kirigami.Heading {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.pageMargin
                Layout.rightMargin: Theme.pageMargin
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
                spacing: Theme.inlineSpacing
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
                        // Tap = "show me how to play this one." The VM
                        // selects the episode (kicking off the streams
                        // fetch) and emits `streamsRequested`, which
                        // MainController forwards to the shell to push
                        // `StreamsPage` on top of this page.
                        seriesDetailVm.selectEpisodeAndOpenStreams(index);
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

        // Bottom breathing room so the last rail clears the scroll edge.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.pageBottomSpacing
        }
    }

    // ---- Error -----------------------------------------------------
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.detailPlaceholderMaxWidth)
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

    // ---- Loading ---------------------------------------------------
    Kirigami.LoadingPlaceholder {
        anchors.centerIn: parent
        visible: seriesDetailVm.metaState === SeriesDetailViewModel.Loading
    }
}
