// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// "More like this" carousel. Mirrors `ContentRail.qml`'s shape but
// without the "Show all" affordance \u2014 the similar list is one
// rail with a cap, not a full Browse view. Backed by the same
// `DiscoverSectionModel` Discover uses, so the rail's Loading /
// Empty / Error placeholders are picked up automatically.
ColumnLayout {
    id: carousel

    /// `DiscoverSectionModel*` exposed by the detail view-model.
    property var sourceModel
    /// Emitted when the user activates a card. Hosts route this
    /// into a `MovieDetailViewModel.activateSimilar(row)` call.
    signal itemActivated(int row)
    // Per-row context menu signals forwarded from the embedded
    // `PosterCard`. Hosts (`MovieDetailPage` / `SeriesDetailPage`)
    // wire these to the matching `*Similar*` slots on the detail
    // view-model.
    signal findStreamsRequested(int row)
    signal addToLibraryRequested(int row)
    signal markWatchedRequested(int row)

    spacing: Theme.inlineSpacing

    Kirigami.Heading {
        Layout.fillWidth: true
        Layout.leftMargin: Theme.pageMargin
        Layout.rightMargin: Theme.pageMargin
        level: 3
        text: sourceModel ? sourceModel.title : ""
        color: Theme.foreground
    }

    ListView {
        id: list
        Layout.fillWidth: true
        Layout.preferredHeight: Math.round(Theme.posterMin * 1.5)
            + Kirigami.Units.gridUnit * 3
        orientation: ListView.Horizontal
        model: sourceModel
        clip: true
        spacing: Theme.groupSpacing
        cacheBuffer: Theme.posterMin * 4
        boundsBehavior: Flickable.StopAtBounds
        leftMargin: Theme.pageMargin
        rightMargin: Theme.pageMargin

        delegate: PosterCard {
            width: Theme.posterMin
            height: list.height
            posterUrl: model.posterUrl !== undefined ? model.posterUrl : ""
            title:     model.title    !== undefined ? model.title    : ""
            year:      model.year !== undefined ? model.year : 0
            rating: (model.voteAverage !== undefined
                && model.voteAverage !== null)
                ? model.voteAverage : -1
            tmdbId: model.tmdbId !== undefined ? model.tmdbId : 0
            kindIndex: model.kind !== undefined ? model.kind : 0
            onClicked: carousel.itemActivated(index)
            onFindStreamsRequested: carousel.findStreamsRequested(index)
            onAddToLibraryRequested: carousel.addToLibraryRequested(index)
            onMarkWatchedRequested: carousel.markWatchedRequested(index)
        }
    }
}
