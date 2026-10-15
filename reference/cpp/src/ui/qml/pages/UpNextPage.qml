// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Up Next page. Vertical stack of horizontal rails sourced from
// playback history and the user's Library:
//
//   * Continue Watching — in-progress playback (history-driven,
//                          includes movies and non-library titles).
//                          Backed by `continueWatchingVm`.
//   * Ready to Watch    — saved series with an aired next-unwatched
//                          episode and no in-progress history yet.
//                          Library-driven (VM rail id "upNext").
//   * Airing Soon       — upcoming episodes for saved series.
//   * Recently Added    — most recently saved titles.
//
// Each rail self-hides when its model is empty. No filter chrome
// lives here — filters are the Library page's concern. Library
// rails are backed by `LibraryViewModel` (shared with the Library
// page); Continue Watching is backed by `ContinueWatchingViewModel`.
//
// Shell mirrors Discover: a `Kirigami.ScrollablePage` hosting a
// `ColumnLayout` of horizontal rails. The page supplies the vertical
// Flickable + Kirigami-styled scrollbar; each rail's inner `ListView`
// flicks horizontally. The shared `PageHeaderBar` provides the title +
// page-actions strip so the chrome matches Discover, Browse, etc.
Kirigami.ScrollablePage {
    id: page

    objectName: "upnext"
    title: i18nc("@title:window", "Up Next")

    leftPadding: 0
    rightPadding: 0
    // Match Discover's first-rail offset so headings line up
    // vertically between the two pages.
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
            onTriggered: libraryVm.refresh()
        }
    ]

    // ---- empty-library placeholder (no titles saved) -----------
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: libraryVm.libraryEmpty
        icon.source: AppIcons.url("library")
        icon.color: AppIcons.foreground
        text: i18nc("@info placeholder", "Your Library is empty.")
        explanation: i18nc("@info placeholder",
            "Open a movie or series detail page and add it to your "
            + "Library.")
    }

    // ---- body: vertical stack of horizontal rails --------------
    ColumnLayout {
        id: rails
        width: page.width
        spacing: Theme.sectionSpacing
        visible: !libraryVm.libraryEmpty

        // History-driven hero. Hidden when history is empty so the
        // page reflows around the remaining library rails.
        ContinueWatchingRail {
            Layout.fillWidth: true
            visible: !continueWatchingVm.empty
        }

        LibraryRail {
            Layout.fillWidth: true
            // User-facing title is "Ready to Watch"; the VM-internal
            // rail id stays "upNext" (see
            // `LibraryViewModel::activateRail`). The rail is dedup'd
            // against Continue Watching: any series whose next-up
            // episode has an in-progress history entry is excluded
            // so the two rails never duplicate a row.
            title: i18nc("@title library rail", "Ready to Watch")
            artworkShape: "thumbnail"
            model: libraryVm.upNextModel
            visible: !!(libraryVm.upNextModel
                && libraryVm.upNextModel.count > 0)
            onItemActivated: row =>
                libraryVm.activateRail("upNext", row)
            onContextMenuRequested:
                (row, rowTitle, rowImdbId, kindIndex) => {
                    upNextMenu.targetRow = row;
                    upNextMenu.rowTitle = rowTitle;
                    upNextMenu.rowImdbId = rowImdbId;
                    upNextMenu.kindIndex = kindIndex;
                    upNextMenu.popup();
                }
        }
        LibraryRail {
            Layout.fillWidth: true
            title: i18nc("@title library rail", "Airing Soon")
            artworkShape: "thumbnail"
            model: libraryVm.airingSoonModel
            visible: !!(libraryVm.airingSoonModel
                && libraryVm.airingSoonModel.count > 0)
            onItemActivated: row =>
                libraryVm.activateRail("airingSoon", row)
            onContextMenuRequested:
                (row, rowTitle, rowImdbId, kindIndex) => {
                    airingSoonMenu.targetRow = row;
                    airingSoonMenu.rowTitle = rowTitle;
                    airingSoonMenu.rowImdbId = rowImdbId;
                    airingSoonMenu.kindIndex = kindIndex;
                    airingSoonMenu.popup();
                }
        }
        LibraryRail {
            Layout.fillWidth: true
            title: i18nc("@title library rail", "Recently Added")
            artworkShape: "poster"
            model: libraryVm.recentlyAddedModel
            visible: !!(libraryVm.recentlyAddedModel
                && libraryVm.recentlyAddedModel.count > 0)
            onItemActivated: row =>
                libraryVm.activateRail("recentlyAdded", row)
            onContextMenuRequested:
                (row, rowTitle, rowImdbId, kindIndex) => {
                    recentlyAddedMenu.targetRow = row;
                    recentlyAddedMenu.rowTitle = rowTitle;
                    recentlyAddedMenu.rowImdbId = rowImdbId;
                    recentlyAddedMenu.kindIndex = kindIndex;
                    recentlyAddedMenu.popup();
                }
        }

        // Smart-rail context menus. Three instances (one per rail)
        // so each menu can carry the rail's id when dispatching to
        // `libraryVm`. No destructive footer — these are computed
        // views and the user removes via the Library grid.

        // Ready to Watch: card-click jumps straight to streams
        // ("resume rail" behavior in `LibraryViewModel::activate
        // Rail`). The menu mirrors that as its primary `Streams`
        // entry and exposes `Details` as the alternate path back
        // to the detail page.
        EpisodeRailContextMenu {
            id: upNextMenu
            primaryLabel: i18nc("@action:inmenu", "Streams")
            primaryIcon: "list-video"
            streamsVisible: false
            detailsVisible: true
            onPrimaryTriggered:
                libraryVm.findStreamsForRailRow("upNext",
                    upNextMenu.targetRow)
            onOpenDetailsTriggered:
                libraryVm.openRailRowDetail("upNext",
                    upNextMenu.targetRow)
            onMarkWatchedTriggered:
                libraryVm.markRailRowWatched("upNext",
                    upNextMenu.targetRow)
        }
        // Airing Soon rows are unaired; `Mark Watched` is hidden
        // because it would be semantically wrong. `Streams` stays
        // available -- streams can occasionally appear a few hours
        // before the official local-time release.
        EpisodeRailContextMenu {
            id: airingSoonMenu
            primaryLabel: i18nc("@action:inmenu", "Details")
            primaryIcon: "info"
            markWatchedVisible: false
            onPrimaryTriggered:
                libraryVm.activateRail("airingSoon",
                    airingSoonMenu.targetRow)
            onFindStreamsTriggered:
                libraryVm.findStreamsForRailRow("airingSoon",
                    airingSoonMenu.targetRow)
        }
        EpisodeRailContextMenu {
            id: recentlyAddedMenu
            primaryLabel: i18nc("@action:inmenu", "Details")
            primaryIcon: "info"
            onPrimaryTriggered:
                libraryVm.activateRail("recentlyAdded",
                    recentlyAddedMenu.targetRow)
            onFindStreamsTriggered:
                libraryVm.findStreamsForRailRow("recentlyAdded",
                    recentlyAddedMenu.targetRow)
            onMarkWatchedTriggered:
                libraryVm.markRailRowWatched("recentlyAdded",
                    recentlyAddedMenu.targetRow)
        }

        // Fallback when the user has saved titles but no rail
        // has anything to show right now.
        Kirigami.PlaceholderMessage {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Theme.sectionSpacing
            Layout.preferredWidth: Math.min(page.width
                    - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            visible: !!((!continueWatchingVm
                    || continueWatchingVm.empty)
                && (!libraryVm.upNextModel
                    || libraryVm.upNextModel.count === 0)
                && (!libraryVm.airingSoonModel
                    || libraryVm.airingSoonModel.count === 0)
                && (!libraryVm.recentlyAddedModel
                    || libraryVm.recentlyAddedModel.count === 0))
            icon.source: AppIcons.url("sparkles")
            icon.color: AppIcons.foreground
            text: i18nc("@info placeholder",
                "Nothing to surface right now")
            explanation: i18nc("@info placeholder",
                "Add some movies or shows to your Library to "
                + "see what's up next, airing soon, or recently "
                + "added here.")
        }
    }
}
