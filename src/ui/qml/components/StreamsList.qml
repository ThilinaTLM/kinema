// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Streams region for the streams page: a quick-filter bar pinned to
// the top, then a state-switched body that renders one of the five
// `StreamsListModel.State` cases (Loading / Ready / Empty / Error /
// Unreleased).
//
// Sort UX lives on the parent page (`Kirigami.Action` opens
// `StreamSortMenu`); the list itself is unaware of how the rows
// arrived in their current order. When the active sort is `Smart`
// or `Quality` the inner `ListView` groups rows under
// `Kirigami.ListSectionHeader` per-resolution; for other sorts the
// list renders flat.
ColumnLayout {
    id: streams

    /// View-model exposing `streams` (StreamsListModel*),
    /// `cachedOnly`, `realDebridConfigured`, `rawStreamsCount`,
    /// `sortMode`, and the new `uiResolutionFilter` /
    /// `uiHdrOnly` / `uiDolbyVisionOnly` / `uiMultiAudioOnly`
    /// transient filters. Defaults to `movieDetailVm`.
    property var vm: movieDetailVm

    spacing: 0

    // ---- top: quick-filter chip strip --------------------------
    StreamFilterBar {
        Layout.fillWidth: true
        vm: streams.vm
    }

    // Subtle separator between the filter bar and the list, so the
    // sticky bar reads as a distinct region rather than floating.
    Kirigami.Separator {
        Layout.fillWidth: true
    }

    // ---- state-switched body ----------------------------------
    StackLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true

        // The model state enum order is Idle / Loading / Ready /
        // Empty / Error / Unreleased \u2014 the indices below mirror it
        // 1:1.
        currentIndex: streams.vm.streams
            ? streams.vm.streams.state
            : 0

        // 0 \u2014 Idle (no title loaded yet).
        Item { }

        // 1 \u2014 Loading.
        Item {
            QQC2.BusyIndicator {
                anchors.centerIn: parent
                running: parent.visible
            }
        }

        // 2 \u2014 Ready: virtualised list of StreamCards, optionally
        // grouped by resolution under section headers.
        ListView {
            id: list
            model: streams.vm.streams
            clip: true
            spacing: 0
            cacheBuffer: Kirigami.Units.gridUnit * 20
            boundsBehavior: Flickable.StopAtBounds

            // Section grouping: only meaningful when rows are
            // ordered by quality (Smart / Quality sorts). For
            // other sorts we render a flat list \u2014 grouping by
            // resolution under a "Provider" sort would just spam
            // headers.
            section.property: (streams.vm.sortMode === StreamsListModel.Smart
                    || streams.vm.sortMode === StreamsListModel.Quality)
                ? "resolution" : ""
            section.criteria: ViewSection.FullString
            section.delegate: Kirigami.ListSectionHeader {
                width: ListView.view ? ListView.view.width : implicitWidth
                text: (section === "\u2014" || section === "")
                    ? i18nc("@title:section unknown resolution", "Other")
                    : section.toUpperCase()
            }

            delegate: StreamCard {
                row: index
                releaseName: model.releaseName
                summaryLine: model.summaryLine
                tags: model.tags
                sizeText: model.sizeText
                seeders: model.seeders
                provider: model.provider
                rdCached: model.rdCached
                rdDownload: model.rdDownload
                hasMagnet: model.hasMagnet
                hasDirectUrl: model.hasDirectUrl
                resolution: model.resolution
                vm: streams.vm
            }
        }

        // 3 \u2014 Empty (no rows after filters / upstream returned 0).
        Kirigami.PlaceholderMessage {
            Layout.alignment: Qt.AlignCenter
            Layout.preferredWidth: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "edit-find"
            text: i18nc("@info placeholder", "No streams")
            explanation: streams.vm.streams
                ? streams.vm.streams.emptyExplanation : ""
            // When the user has narrowed via the filter chips, give
            // them a one-click out instead of forcing a manual untoggle
            // of each chip.
            helpfulAction: streams.vm.uiAnyFilterActive
                ? clearFiltersAction : null
            Kirigami.Action {
                id: clearFiltersAction
                icon.name: "edit-clear-all"
                text: i18nc("@action:button reset stream filters", "Reset filters")
                onTriggered: streams.vm.clearUiFilters()
            }
        }

        // 4 \u2014 Error.
        Kirigami.PlaceholderMessage {
            Layout.alignment: Qt.AlignCenter
            Layout.preferredWidth: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "dialog-error"
            text: i18nc("@info placeholder", "Couldn't fetch streams")
            explanation: streams.vm.streams
                ? streams.vm.streams.errorMessage : ""
            helpfulAction: Kirigami.Action {
                icon.name: "view-refresh"
                text: i18nc("@action:button", "Retry")
                onTriggered: streams.vm.retry()
            }
        }

        // 5 \u2014 Unreleased.
        Kirigami.PlaceholderMessage {
            Layout.alignment: Qt.AlignCenter
            Layout.preferredWidth: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "appointment-soon"
            text: i18nc("@info placeholder",
                "Not released yet")
            explanation: (streams.vm.streams
                && streams.vm.streams.releaseDate
                && streams.vm.streams.releaseDate.toString().length > 0)
                ? i18nc("@info placeholder body. %1 is a localized date",
                    "Streams will be available from %1.",
                    Qt.formatDate(
                        streams.vm.streams.releaseDate, Qt.DefaultLocaleLongDate))
                : ""
        }
    }
}
