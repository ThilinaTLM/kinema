// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Discover home: a stack of TMDB-backed content rails (Trending,
// Popular Series, Now Playing, On The Air, Top Rated Movies, Top
// Rated Series). Activity-derived rails (Continue Watching, Ready
// to Watch, Airing Soon, Recently Added) live on the Up Next page.
//
// Empty / not-configured / auth-failed: a `Kirigami.PlaceholderMessage`
// fills the page with an "Open Settings" call-to-action — same hook
// the legacy widget DiscoverPage exposed.
//
// The shared `PageHeaderBar` (see `components/PageHeaderBar.qml`)
// provides the title + page-actions strip so this page wears the same
// chrome as Browse, Search, Library, etc.
//
// Backed by the `discoverVm` context property installed on the
// engine by `MainController::exposeContextProperties`.
Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title:window", "Discover")
    objectName: "discover"

    leftPadding: 0
    rightPadding: 0
    // Keep the first rail offset consistent with the spacing
    // between rails, so the first section doesn't sit tighter to
    // the page header than the rest of the sections do.
    topPadding: Theme.sectionSpacing
    bottomPadding: Theme.pageBottomSpacing

    header: PageHeaderBar {
        title: page.title
        pageActions: page.actions

        // Right-edge alignment of page actions; no inline filter
        // widgets on this page.
        Item { Layout.fillWidth: true }
    }

    actions: [
        Kirigami.Action {
            icon.source: AppIcons.url("refresh-cw")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button", "Refresh")
            displayHint: Kirigami.DisplayHint.IconOnly
            shortcut: StandardKey.Refresh
            onTriggered: discoverVm.refresh()
        }
    ]

    // ---- Empty / not-configured / auth-failed -----------------------
    // Owns the entire page surface when active; the rails below stay
    // hidden so we don't paint two competing states.
    Kirigami.PlaceholderMessage {
        id: placeholder
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: !discoverVm.tmdbConfigured || discoverVm.authFailed

        icon.source: AppIcons.url(discoverVm.authFailed ? "triangle-alert" : "settings")
        icon.color: discoverVm.authFailed ? AppIcons.negative : AppIcons.foreground
        text: discoverVm.placeholderTitle
        explanation: discoverVm.placeholderBody

        helpfulAction: Kirigami.Action {
            icon.source: AppIcons.url("settings")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button", "Open settings…")
            onTriggered: shell.requestSettings()
        }
    }

    // ---- Rails ----------------------------------------------------
    ColumnLayout {
        id: stack
        width: page.width
        spacing: Theme.sectionSpacing
        visible: discoverVm.tmdbConfigured && !discoverVm.authFailed

        // TMDB rails. The Repeater iterates over `discoverVm.sections`
        // (a `QList<QObject*>` of `DiscoverSectionModel*`s) and
        // forwards activation back through the VM with the section
        // index so it can resolve the right item.
        Repeater {
            model: discoverVm.sections

            delegate: ContentRail {
                required property var modelData
                required property int index

                Layout.fillWidth: true
                sectionModel: modelData
                onItemActivated: function (row) {
                    discoverVm.activateItem(index, row);
                }
                onShowAllRequested: discoverVm.browseSection(index)
                onFindStreamsRequested: function (row) {
                    discoverVm.findStreamsForRow(index, row);
                }
                onAddToLibraryRequested: function (row) {
                    discoverVm.addRowToLibrary(index, row);
                }
                onMarkWatchedRequested: function (row) {
                    discoverVm.markRowWatched(index, row);
                }
            }
        }
    }
}
