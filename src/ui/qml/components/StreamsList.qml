// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Body-only streams region: a `ListSurface` that renders one of the
// six `StreamsListModel.State` cases (Idle / Loading / Ready / Empty
// / Error / Unreleased).
//
// Filter chrome (resolution, HDR, DV, multi-audio, cached, sort)
// lives on the parent `StreamsPage` header `ActionToolBar`; this list
// is unaware of how the rows arrived in their current order. When
// the active sort is `Smart` or `Quality` the inner `ListView` groups
// rows under `Kirigami.ListSectionHeader` per-resolution; for other
// sorts the list renders flat.
ListSurface {
    id: streams

    /// View-model exposing `streams` (StreamsListModel*),
    /// `debridConfigured`, `rawStreamsCount`, `sortMode`, and
    /// the `uiResolutionFilter` / `uiHdrOnly` / `uiDolbyVisionOnly`
    /// / `uiMultiAudioOnly` transient filters. Defaults to
    /// `movieDetailVm`.
    property var vm: movieDetailVm

    // Map the C++ `StreamsListModel::State` enum (numeric, Q_ENUM)
    // into the `ListSurface.State` QML enum. Indices align with the
    // model order so the cast keeps a 1:1 mapping; Unreleased is the
    // only branch that needs the `Custom` escape hatch since its
    // placeholder doesn't fit the title / explanation / action
    // shape that empty / error use.
    state: {
        if (!streams.vm || !streams.vm.streams) {
            return ListSurface.Idle;
        }
        switch (streams.vm.streams.state) {
        case StreamsListModel.Idle:       return ListSurface.Idle;
        case StreamsListModel.Loading:    return ListSurface.Loading;
        case StreamsListModel.Ready:      return ListSurface.Ready;
        case StreamsListModel.Empty:      return ListSurface.Empty;
        case StreamsListModel.Error:      return ListSurface.Error;
        case StreamsListModel.Unreleased: return ListSurface.Custom;
        }
        return ListSurface.Idle;
    }

    model: streams.vm ? streams.vm.streams : null

    sectionProperty: (streams.vm
            && (streams.vm.sortMode === StreamsListModel.Smart
                || streams.vm.sortMode === StreamsListModel.Quality))
        ? "resolution" : ""
    sectionCriteria: ViewSection.FullString
    sectionDelegate: Kirigami.ListSectionHeader {
        // Match the delegate's effective width so the section
        // separator line ends where cards end, not where the
        // ListView's geometry ends (which extends under the
        // overlay scrollbar gutter).
        width: ListView.view
            ? ListView.view.width
                - ListView.view.leftMargin
                - ListView.view.rightMargin
            : implicitWidth
        text: (section === "\u2014" || section === "")
            ? i18nc("@title:section unknown resolution", "Other")
            : section.toUpperCase()
    }

    delegate: StreamListCard {
        required property int index
        required property var model
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
        debridProvider: model.debridProvider
        debridCached: model.debridCached
        vm: streams.vm
    }

    emptyConfig: ({
        icon: "search",
        iconColor: AppIcons.foreground,
        title: i18nc("@info placeholder", "No streams"),
        explanation: streams.vm && streams.vm.streams
            ? streams.vm.streams.emptyExplanation : "",
        actionText: streams.vm && streams.vm.uiAnyFilterActive
            ? i18nc("@action:button reset stream filters",
                "Reset filters")
            : "",
        actionIcon: "eraser",
        onActionTriggered: () => {
            if (streams.vm) streams.vm.clearUiFilters();
        }
    })

    errorConfig: ({
        icon: "circle-alert",
        iconColor: AppIcons.negative,
        title: i18nc("@info placeholder",
            "Couldn't fetch streams"),
        explanation: streams.vm && streams.vm.streams
            ? streams.vm.streams.errorMessage : "",
        actionText: i18nc("@action:button", "Retry"),
        actionIcon: "refresh-cw",
        onActionTriggered: () => {
            if (streams.vm) streams.vm.retry();
        }
    })

    customComponent: Component {
        Kirigami.PlaceholderMessage {
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            icon.source: AppIcons.url("clock-arrow-down")
            icon.color: AppIcons.foreground
            text: i18nc("@info placeholder",
                "Not released yet")
            explanation: (streams.vm
                && streams.vm.streams
                && streams.vm.streams.releaseDate
                && streams.vm.streams.releaseDate
                    .toString().length > 0)
                ? i18nc(
                    "@info placeholder body. %1 is a localized date",
                    "Streams will be available from %1.",
                    Qt.formatDate(streams.vm.streams.releaseDate,
                        Qt.DefaultLocaleLongDate))
                : ""
        }
    }
}
