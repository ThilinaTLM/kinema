// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Responsive grid of `PosterCard`s. Used by Search and Browse with
// different list models — each model exposes the same role names
// (`title`, `posterUrl`, `year`, `rating`) so the delegate is shared.
//
// `GridView` reflows to N columns based on the available width;
// `cellWidth` scales with `Theme.posterMin` so the grid keeps a
// consistent card size across the screen sizes Kirigami targets.
//
// `onItemActivated(int row)` fires on click; consumers route it to
// `searchVm.activate(row)` / `browseVm.activate(row)`.
GridView {
    id: grid

    /// Required: a QAbstractListModel exposing the role names
    /// `posterUrl`, `title`, `year`, `rating`.
    property var sourceModel
    /// Optional sentinel emitted when the user reaches the last row.
    /// Used by Browse for pagination; Search ignores it.
    signal nearEndOfList()
    signal itemActivated(int row)

    model: sourceModel
    cellWidth: Theme.posterMax + Kirigami.Units.largeSpacing
    cellHeight: Math.round(Theme.posterMax * 1.6)
    clip: true
    cacheBuffer: cellHeight * 2
    boundsBehavior: Flickable.StopAtBounds

    // Approximate end-of-list detector for "Load more" sentinels.
    // Browse listens to this; Search does not.
    onAtYEndChanged: if (atYEnd && count > 0) grid.nearEndOfList()

    delegate: PosterCard {
        width: grid.cellWidth - Kirigami.Units.largeSpacing
        height: grid.cellHeight - Kirigami.Units.largeSpacing
        posterUrl: model.posterUrl !== undefined ? model.posterUrl : ""
        title:     model.title    !== undefined ? model.title    : ""
        subtitle:  (model.year !== undefined && model.year > 0)
            ? String(model.year) : ""
        rating: (model.rating !== undefined && model.rating !== null)
            ? model.rating
            : ((model.voteAverage !== undefined
                && model.voteAverage !== null)
                ? model.voteAverage : -1)
        onClicked: grid.itemActivated(index)
    }
}
