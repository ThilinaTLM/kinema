// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Pushed-on-top streams surface. Each detail page (movie / series) reaches
// this page through `MainController::showStreamsRequested(detailVm)`. The
// page is VM-agnostic — both `MovieDetailViewModel` and
// `SeriesDetailViewModel` expose the same contract used by `StreamsList`
// and `StreamSortMenu` (`streams`, `cachedOnly`, `realDebridConfigured`,
// `rawStreamsCount`, `sortMode`, `sortDescending`, `retry()`,
// `requestSubtitles()`).
//
// Esc pops back to the detail page (handled by the shell-level shortcut in
// `ApplicationShell.qml`).
Kirigami.Page {
    id: page

    objectName: "streams"
    padding: 0

    // Set by the shell when the page is created. Kept as `var` rather than
    // a typed property so QML accepts both detail VM types interchangeably.
    property var detailVm

    title: page.detailVm
        ? (page.detailVm.selectedEpisodeLabel !== undefined
            && page.detailVm.selectedEpisodeLabel.length > 0
            // Series — `selectedEpisodeLabel` already includes show title.
            ? i18nc("@title:window streams page, %1 is a series episode label",
                "Streams \u2014 %1", page.detailVm.selectedEpisodeLabel)
            : i18nc("@title:window streams page, %1 is a movie title",
                "Streams \u2014 %1", page.detailVm.title))
        : i18nc("@title:window streams page fallback",
            "Streams")

    actions: [
        Kirigami.Action {
            icon.name: "view-sort-ascending"
            text: i18nc("@action:button", "Sort streams")
            displayHint: Kirigami.DisplayHint.IconOnly
                | Kirigami.DisplayHint.KeepVisible
            enabled: page.detailVm
                && page.detailVm.streams
                && page.detailVm.streams.count > 0
            onTriggered: sortMenu.popup()
        },
        Kirigami.Action {
            icon.name: "subtitles"
            text: i18nc("@action:button", "Subtitles\u2026")
            enabled: page.detailVm !== undefined
                && page.detailVm !== null
            onTriggered: page.detailVm.requestSubtitles()
        }
    ]

    StreamsList {
        anchors.fill: parent
        vm: page.detailVm
    }

    StreamSortMenu {
        id: sortMenu
        vm: page.detailVm
    }
}
