// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Pushed-on-top streams surface. Each detail page (movie / series)
// reaches this page through `MainController::showStreamsRequested(detailVm)`.
// The page is VM-agnostic \u2014 both `MovieDetailViewModel` and
// `SeriesDetailViewModel` expose the same contract used by
// `StreamsList` and `StreamSortMenu`.
//
// Esc pops back to the detail page (handled by the shell-level
// shortcut in `ApplicationShell.qml`).
Kirigami.Page {
    id: page

    objectName: "streams"
    padding: 0

    // Set by the shell when the page is created. Kept as `var` rather
    // than a typed property so QML accepts both detail VM types
    // interchangeably.
    property var detailVm

    // Title carries the title / episode label, with the visible-row
    // count appended after a separator when there are streams. The
    // legacy `%1 streams` header row + Cached-only checkbox have
    // both moved off-screen: the toggle into `StreamFilterBar`, the
    // count into the title here. `Kirigami.Page` doesn't support a
    // `subtitle` property, so we inline it.
    title: {
        if (!page.detailVm) {
            return i18nc("@title:window streams page fallback", "Streams");
        }
        const label = (page.detailVm.selectedEpisodeLabel !== undefined
            && page.detailVm.selectedEpisodeLabel.length > 0)
            // Series \u2014 `selectedEpisodeLabel` already includes show title.
            ? page.detailVm.selectedEpisodeLabel
            : page.detailVm.title;
        if (page.detailVm.streams
            && page.detailVm.streams.count > 0) {
            return i18nc(
                "@title:window streams page, %1 is title, %2 is count",
                "%1 \u00b7 %2 streams", label,
                page.detailVm.streams.count);
        }
        return label;
    }

    actions: [
        Kirigami.Action {
            icon.name: "view-sort"
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
