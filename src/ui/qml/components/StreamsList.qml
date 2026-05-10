// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Body-only streams region: a state-switched container that renders
// one of the six `StreamsListModel.State` cases (Idle / Loading /
// Ready / Empty / Error / Unreleased).
//
// Filter chrome (resolution, HDR, DV, multi-audio, cached, sort) lives
// on the parent `StreamsPage` header `ActionToolBar`; this list is
// unaware of how the rows arrived in their current order. When the
// active sort is `Smart` or `Quality` the inner `ListView` groups rows
// under `Kirigami.ListSectionHeader` per-resolution; for other sorts
// the list renders flat.
StackLayout {
    id: streams

    /// View-model exposing `streams` (StreamsListModel*),
    /// `debridConfigured`, `rawStreamsCount`, `sortMode`, and
    /// the `uiResolutionFilter` / `uiHdrOnly` / `uiDolbyVisionOnly`
    /// / `uiMultiAudioOnly` transient filters.
    /// Defaults to `movieDetailVm`.
    property var vm: movieDetailVm

    // The model state enum order is Idle / Loading / Ready /
    // Empty / Error / Unreleased — the indices below mirror it 1:1.
    currentIndex: streams.vm.streams
        ? streams.vm.streams.state
        : 0

    // 0 — Idle (no title loaded yet).
    Item { }

    // 1 — Loading.
    Item {
        QQC2.BusyIndicator {
            anchors.centerIn: parent
            running: parent.visible
        }
    }

    // 2 — Ready: virtualised list of StreamCards, optionally
    // grouped by resolution under section headers.
    ListView {
        id: list
        model: streams.vm.streams
        clip: true
        spacing: Theme.inlineSpacing
        leftMargin: Theme.pageMargin
        rightMargin: Theme.pageMargin
        topMargin: Theme.inlineSpacing
        bottomMargin: Theme.groupSpacing
        cacheBuffer: Kirigami.Units.gridUnit * 20
        boundsBehavior: Flickable.StopAtBounds

        // Section grouping: only meaningful when rows are ordered by
        // quality (Smart / Quality sorts). For other sorts we render a
        // flat list — grouping by resolution under a "Provider" sort
        // would just spam headers.
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
            hasMagnet: model.hasMagnet
            hasDirectUrl: model.hasDirectUrl
            resolution: model.resolution
            vm: streams.vm
        }
    }

    // 3 — Empty (no rows after filters / upstream returned 0).
    Item {
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            icon.source: AppIcons.url("search")
            icon.color: AppIcons.foreground
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
                icon.source: AppIcons.url("eraser")
                icon.color: AppIcons.foreground
                text: i18nc("@action:button reset stream filters",
                    "Reset filters")
                onTriggered: streams.vm.clearUiFilters()
            }
        }
    }

    // 4 — Error.
    Item {
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            icon.source: AppIcons.url("circle-alert")
            icon.color: AppIcons.negative
            text: i18nc("@info placeholder", "Couldn't fetch streams")
            explanation: streams.vm.streams
                ? streams.vm.streams.errorMessage : ""
            helpfulAction: Kirigami.Action {
                icon.source: AppIcons.url("refresh-cw")
                icon.color: AppIcons.foreground
                text: i18nc("@action:button", "Retry")
                onTriggered: streams.vm.retry()
            }
        }
    }

    // 5 — Unreleased.
    Item {
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            icon.source: AppIcons.url("clock-arrow-down")
            icon.color: AppIcons.foreground
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
