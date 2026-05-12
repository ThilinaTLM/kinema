// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
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

    Kirigami.PromptDialog {
        id: removeLibraryDialog

        title: i18nc("@title:dialog", "Remove from Library?")
        subtitle: i18nc("@info",
            "\u201c%1\u201d will be removed from your Library. "
            + "Your watched and playback history for this series "
            + "is preserved.",
            seriesDetailVm.title)

        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Cancel")
                icon.source: AppIcons.url("x")
                icon.color: AppIcons.foreground
                onTriggered: removeLibraryDialog.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button", "Remove")
                icon.source: AppIcons.url("library")
                icon.color: AppIcons.negative
                onTriggered: {
                    seriesDetailVm.removeFromLibrary();
                    removeLibraryDialog.close();
                }
            }
        ]
    }

    readonly property Kirigami.Action libraryAction: Kirigami.Action {
        icon.source: AppIcons.url("library")
        icon.color: enabled ? AppIcons.foreground : AppIcons.muted
        text: seriesDetailVm.libraryActionText
        displayHint: Kirigami.DisplayHint.IconOnly
            | Kirigami.DisplayHint.KeepVisible
        enabled: seriesDetailVm.metaState === SeriesDetailViewModel.Ready
        onTriggered: {
            if (seriesDetailVm.inLibrary) {
                removeLibraryDialog.open();
            } else {
                seriesDetailVm.addToLibrary();
            }
        }
    }

    // Watched-state is independent of Library membership: a user can
    // mark a series watched / unwatched without saving it. The action
    // is enabled the moment the meta has resolved.
    readonly property Kirigami.Action seriesWatchedAction: Kirigami.Action {
        icon.source: AppIcons.url(seriesDetailVm.seriesWatched ? "circle-dashed" : "circle-check")
        icon.color: enabled ? AppIcons.foreground : AppIcons.muted
        text: seriesDetailVm.seriesWatched
            ? i18nc("@action:button", "Mark Unwatched")
            : i18nc("@action:button", "Mark Watched")
        displayHint: Kirigami.DisplayHint.IconOnly
            | Kirigami.DisplayHint.KeepVisible
        enabled: seriesDetailVm.metaState === SeriesDetailViewModel.Ready
        onTriggered: seriesDetailVm.toggleSeriesWatched()
    }

    readonly property Kirigami.Action refreshAction: Kirigami.Action {
        icon.source: AppIcons.url("refresh-cw")
        icon.color: enabled ? AppIcons.foreground : AppIcons.muted
        text: i18nc("@action:button", "Refresh")
        displayHint: Kirigami.DisplayHint.IconOnly
            | Kirigami.DisplayHint.KeepVisible
        shortcut: StandardKey.Refresh
        enabled: seriesDetailVm.metaState !== SeriesDetailViewModel.Loading
            && seriesDetailVm.imdbId.length > 0
        onTriggered: seriesDetailVm.retry()
    }

    actions: [ refreshAction ]

    header: PageHeaderBar {
        title: page.title
        pageActions: page.actions

        // Right-edge alignment of page actions; no inline filter
        // widgets on this page (the hero owns Library / Watched).
        Item { Layout.fillWidth: true }
    }

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

            primaryAction: page.libraryAction
            secondaryAction: page.seriesWatchedAction
        }

        // The episode list and season tabs already indent their
        // content via internal padding (`EpisodeListCard.padding`,
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

                delegate: EpisodeListCard {
                    required property int index
                    required property var model
                    episodeNumber: model.number
                    episodeTitle: model.title
                    description: model.description
                    releasedText: model.releasedText
                    isUpcoming: model.isUpcoming
                    thumbnailUrl: model.thumbnailUrl
                    watched: model.watched !== undefined
                        ? model.watched : false
                    // Hide the inline progress bar once the episode
                    // has been marked watched — chassis renders the
                    // bar whenever 0 < progress < 1.
                    progress: (model.watched === true)
                        ? -1
                        : (model.progress !== undefined
                            ? model.progress : -1)
                    selected: index === seriesDetailVm.selectedEpisodeRow
                    onToggleWatchedRequested:
                        seriesDetailVm.toggleEpisodeWatched(index)
                    onClicked: {
                        // Tap = "show me how to play this one." The
                        // VM selects the episode (kicking off the
                        // streams fetch) and emits
                        // `streamsRequested`, which MainController
                        // forwards to the shell to push `StreamsPage`
                        // on top of this page.
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
        icon.source: AppIcons.url("circle-alert")
        icon.color: AppIcons.negative
        text: i18nc("@info placeholder", "Couldn't load this series.")
        explanation: seriesDetailVm.metaError
        helpfulAction: Kirigami.Action {
            icon.source: AppIcons.url("refresh-cw")
            icon.color: AppIcons.foreground
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
