// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Responsive grid of `PosterCard`s used by Search and Browse.
//
// `cellWidth` is recomputed from the available width so the grid
// always uses every pixel — no dead band on the right, no clipped
// cards on either edge. `cellHeight` is derived from `cellWidth`
// (poster aspect 2:3) plus a measured meta block, so cards never
// clip their two-line title even at large accessibility font sizes.
//
// Spacing comes from per-cell insets on the delegate; we no longer
// shrink the cell as a workaround for `GridView`'s lack of inter-cell
// spacing.
//
// Public surface:
//   * `sourceModel`        — required QAbstractListModel
//   * `signal nearEndOfList()` — fires when the user reaches the
//     last row (Browse pagination consumer)
//   * `signal itemActivated(int row)` — emitted on click / Enter
GridView {
    id: grid

    /// Required: a QAbstractListModel exposing role names
    /// `posterUrl`, `title`, plus optionally `year` / `rating` /
    /// `voteAverage`.
    property var sourceModel
    signal nearEndOfList()
    signal itemActivated(int row)
    // Per-row context menu signals forwarded from the embedded
    // `PosterCard`. Consumers (`BrowsePage`, `SearchPage`) wire
    // them to the matching VM slot.
    signal findStreamsRequested(int row)
    signal addToLibraryRequested(int row)
    signal markWatchedRequested(int row)

    // Visual breathing room around the whole grid so cards do not
    // kiss the page edge.
    leftMargin:   Theme.pageMargin
    rightMargin:  Theme.pageMargin
    topMargin:    Theme.pageMargin
    bottomMargin: Theme.pageMargin

    // ---- Responsive sizing --------------------------------------
    // The minimum cell footprint a poster wants. We then split the
    // available width into the largest integer number of columns
    // that fit and stretch each cell to absorb the leftover pixels.
    readonly property int targetCellWidth: Theme.posterMin
        + Kirigami.Units.largeSpacing
    readonly property int availableWidth: Math.max(0,
        width - leftMargin - rightMargin)
    readonly property int columns: Math.max(1,
        Math.floor(availableWidth / targetCellWidth))

    // Two title lines + caption + spacing. Recomputed when the user
    // changes their KDE font size. Use real `FontMetrics.lineSpacing`
    // (not `pixelSize`) and ceil it so we budget for line leading +
    // subpixel rounding — must match `BasePosterCard`'s title-height
    // reservation, otherwise the grid cell is too short and the title
    // elides to a single line instead of wrapping.
    FontMetrics {
        id: titleFM
        font: Kirigami.Theme.defaultFont
    }
    FontMetrics {
        id: subtitleFM
        font: Kirigami.Theme.smallFont
    }
    readonly property int metaBlockHeight:
        Math.ceil(titleFM.lineSpacing) * 2
            + Math.ceil(subtitleFM.lineSpacing)
            + Kirigami.Units.smallSpacing * 3

    cellWidth: Math.floor(availableWidth / columns)
    cellHeight: Math.round((cellWidth - cellInset * 2) * 1.5)
        + metaBlockHeight + cellInset * 2

    // Per-cell padding. Each delegate pads itself by `cellInset` on
    // every side so the visible gap between cards is `cellInset * 2`.
    readonly property int cellInset: Kirigami.Units.smallSpacing

    model: sourceModel
    clip: true
    cacheBuffer: cellHeight * 2
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.VerticalFlick

    // Approximate end-of-list detector for "Load more" sentinels.
    onAtYEndChanged: if (atYEnd && count > 0) grid.nearEndOfList()

    delegate: Item {
        // Pad the cell to manufacture the inter-card gap. The visible
        // card sits inside the inset; clicks on the gap pass through.
        width: grid.cellWidth
        height: grid.cellHeight

        PosterCard {
            anchors.fill: parent
            anchors.margins: grid.cellInset

            posterUrl: model.posterUrl !== undefined
                ? model.posterUrl : ""
            title: model.title !== undefined ? model.title : ""
            year: model.year !== undefined ? model.year : 0
            rating: (model.rating !== undefined && model.rating !== null)
                ? model.rating
                : ((model.voteAverage !== undefined
                        && model.voteAverage !== null)
                    ? model.voteAverage : -1)
            // Browse rows carry `tmdbId`; Search rows carry
            // `imdbId` instead. Pass whichever role the underlying
            // model exposes; the menu adapts its "Open on …" item
            // to the available identifier.
            tmdbId: model.tmdbId !== undefined ? model.tmdbId : 0
            imdbId: model.imdbId !== undefined ? model.imdbId : ""
            kindIndex: model.kind !== undefined ? model.kind : 0
            onClicked: grid.itemActivated(index)
            onFindStreamsRequested: grid.findStreamsRequested(index)
            onAddToLibraryRequested: grid.addToLibraryRequested(index)
            onMarkWatchedRequested: grid.markWatchedRequested(index)
        }
    }
}
