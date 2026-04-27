// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Menu of sort options + asc/desc toggle. Bound to the owning
// view-model's `sortMode` (int) and `sortDescending` (bool)
// properties; choosing an item rewrites both. Used by the
// `StreamsPage` header `Sort streams` action.
//
// `Smart` is the default: it puts cached rows first, then orders
// by resolution rank desc, then size desc. The descending toggle
// is meaningless under Smart (its shape is fixed) so the toggle
// item disables itself when Smart is active.
QQC2.Menu {
    id: menu

    /// View-model with `sortMode` (int) + `sortDescending` (bool)
    /// + `sortChanged` notify. Defaults to `movieDetailVm`.
    property var vm: movieDetailVm

    QQC2.MenuItem {
        text: i18nc("@action:inmenu sort streams",
            "Smart (cached, then quality)")
        checkable: true
        checked: menu.vm.sortMode === StreamsListModel.Smart
        onTriggered: menu.vm.sortMode = StreamsListModel.Smart
    }
    QQC2.MenuSeparator { }
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
        // Smart has a fixed shape \u2014 disable the toggle so the user
        // doesn't think they're affecting anything when they aren't.
        enabled: menu.vm.sortMode !== StreamsListModel.Smart
        onTriggered: menu.vm.sortDescending = !menu.vm.sortDescending
    }
}
