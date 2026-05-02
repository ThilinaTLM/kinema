// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// List view of the Library page. Filterable poster grid of
// `LibraryCard`s driven by `libraryVm.model` (the C++ side
// already applies kind/status/genre/sort/date/rating axes when
// publishing the rows).
//
// The "no matches" placeholder is part of the view (not the page)
// so the empty grid gets the right empty state without the page
// having to switch its outer placeholder by mode.
Item {
    id: view

    /// Caller wires this to `libraryVm`. Named `vm` (not
    /// `libraryVm`) so the caller's `vm: libraryVm` binding
    /// doesn't self-reference: a property named `libraryVm` on
    /// this object would shadow the context property of the same
    /// name when QML resolves the RHS against the scope object.
    property var vm

    /// Emitted when the card's "Remove from Library" menu fires;
    /// the parent page hosts the confirmation dialog so the row
    /// index is captured at the page level.
    signal removeRequested(int row, string title)

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: !!(view.vm && view.vm.empty)
        icon.source: AppIcons.url("search")
        icon.color: AppIcons.foreground
        text: i18nc("@info placeholder", "No matches")
        explanation: i18nc("@info placeholder",
            "Nothing in your Library matches these filters. Try "
            + "clearing some.")
        helpfulAction: Kirigami.Action {
            icon.source: AppIcons.url("eraser")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button", "Reset filters")
            onTriggered: view.vm && view.vm.resetFilters()
        }
    }

    GridView {
        id: grid
        anchors.fill: parent
        visible: !!(view.vm && !view.vm.empty)
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        cacheBuffer: cellHeight * 2

        model: view.vm ? view.vm.model : null

        leftMargin: Theme.pageMargin
        rightMargin: Theme.pageMargin
        topMargin: Theme.pageMargin
        bottomMargin: Theme.pageBottomSpacing

        readonly property int targetCellWidth:
            Theme.posterMin + Kirigami.Units.largeSpacing
        readonly property int availableWidth: Math.max(0,
            width - leftMargin - rightMargin)
        readonly property int columns: Math.max(1,
            Math.floor(availableWidth / targetCellWidth))
        readonly property int cellInset: Kirigami.Units.smallSpacing

        cellWidth: Math.floor(availableWidth / columns)
        cellHeight: Math.round((cellWidth - cellInset * 2) * 1.5)
            + Kirigami.Theme.defaultFont.pixelSize * 4

        delegate: Item {
            width: grid.cellWidth
            height: grid.cellHeight

            LibraryCard {
                anchors.fill: parent
                anchors.margins: grid.cellInset
                posterUrl: model.posterUrl !== undefined
                    ? model.posterUrl : ""
                title: model.title !== undefined ? model.title : ""
                subtitle: model.subtitle !== undefined ? model.subtitle : ""
                rating: (model.rating !== undefined && model.rating !== null)
                    ? model.rating : -1
                progress: model.progress !== undefined ? model.progress : -1
                watched: model.watched !== undefined ? model.watched : false
                upcoming: model.upcoming !== undefined ? model.upcoming : false
                onClicked: view.vm && view.vm.activate(index)
                onRemoveRequested: view.removeRequested(index,
                    model.title !== undefined ? model.title : "")
                onToggleWatchedRequested:
                    view.vm && view.vm.toggleWatched(index)
            }
        }
    }
}
