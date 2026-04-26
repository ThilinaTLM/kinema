// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Menu of sort options + asc/desc toggle. Bound to the owning
// view-model's `sortMode` (int) and `sortDescending` (bool)
// properties; choosing an item rewrites both. Used by
// `MovieDetailPage`'s header `Sort streams` action.
//
// The enum values come from `StreamsListModel.SortMode.*` so QML
// reads them by name rather than hard-coded integers.
QQC2.Menu {
    id: menu

    /// View-model with `sortMode` (int) + `sortDescending` (bool)
    /// + `sortChanged` notify. Defaults to `movieDetailVm`; commit
    /// B's SeriesDetailPage rebinds it.
    property var vm: movieDetailVm

    QQC2.MenuItem {
        text: i18nc("@action:inmenu sort streams", "Seeders")
        checkable: true
        checked: menu.vm.sortMode === StreamsListModel.Seeders
        onTriggered: menu.vm.sortMode = StreamsListModel.Seeders
    }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu sort streams", "Size")
        checkable: true
        checked: menu.vm.sortMode === StreamsListModel.Size
        onTriggered: menu.vm.sortMode = StreamsListModel.Size
    }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu sort streams", "Quality")
        checkable: true
        checked: menu.vm.sortMode === StreamsListModel.Quality
        onTriggered: menu.vm.sortMode = StreamsListModel.Quality
    }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu sort streams", "Provider")
        checkable: true
        checked: menu.vm.sortMode === StreamsListModel.Provider
        onTriggered: menu.vm.sortMode = StreamsListModel.Provider
    }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu sort streams", "Release name")
        checkable: true
        checked: menu.vm.sortMode === StreamsListModel.ReleaseName
        onTriggered: menu.vm.sortMode = StreamsListModel.ReleaseName
    }
    QQC2.MenuSeparator { }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu sort streams", "Descending")
        checkable: true
        checked: menu.vm.sortDescending
        onTriggered: menu.vm.sortDescending = !menu.vm.sortDescending
    }
}
