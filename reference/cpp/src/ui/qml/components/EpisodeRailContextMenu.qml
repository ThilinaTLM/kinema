// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2

import dev.tlmtech.kinema.app

// Shared context menu for every Library-page rail backed by
// `EpisodeRailCard` (Continue Watching, Up Next, Airing Soon,
// Recently Added).
//
// The four rails share a common skeleton -- a configurable primary
// item, a configurable secondary group (Streams jump + Details
// nav, each independently togglable), a configurable state group
// (Mark Watched), the always-on Copy Title / Open on IMDb pair,
// and an optional destructive footer Continue Watching uses for
// "Remove".
//
// Defaults reflect the "primary action opens the detail page"
// shape used by Airing Soon and Recently Added: primary =
// `Details` + `info` icon, `streamsVisible` true,
// `detailsVisible` false, `markWatchedVisible` true. Rails whose
// primary skips the detail page (Continue Watching, Ready to
// Watch) opt in to `detailsVisible` and override the primary;
// Airing Soon opts out of `markWatchedVisible` because its rows
// are unaired.
//
// Consumer contract:
//
//   * Set `targetRow` before `popup()`. The signals carry no
//     args; consumers read `targetRow` to identify the row.
//   * `primaryLabel` / `primaryIcon` flip the topmost item from
//     `Details` (default) to `Resume` / `Streams` / etc.
//   * `streamsVisible`, `detailsVisible`, `markWatchedVisible`
//     toggle the matching items; hidden items collapse cleanly
//     because their adjacent separators are bound to the same
//     flag.
//   * `removeLabel` empty hides the destructive footer entirely;
//     non-empty surfaces it as a `destructive: true` item that
//     emits `confirmRemoveRequested`. The consumer is responsible
//     for attaching its own `Kirigami.PromptDialog`.
//
// Idempotent actions (Copy Title, Open on IMDb) call the shell
// helpers directly so the menu doesn't need a per-rail VM pointer
// for them.
KinemaMenu {
    id: menu

    /// Row index inside the consumer's `LibraryRailModel`. Read
    /// from the consumer's signal handlers.
    property int targetRow: -1

    /// Title + IMDb id of the row that triggered the menu. Set
    /// from the model row (`primaryLine` / `title` / `imdbId`)
    /// before calling `popup()`.
    property string rowTitle: ""
    property string rowImdbId: ""
    /// `domain::MediaKind` value (0 = Movie, 1 = Series).
    property int kindIndex: 0

    /// Primary-item label / icon. Defaults to `Details` + `info`
    /// (the detail-page-nav shape used by Airing Soon / Recently
    /// Added). Continue Watching sets `Resume` + `play`; Ready to
    /// Watch sets `Streams` + `list-video`.
    property string primaryLabel:
        i18nc("@action:inmenu episode rail", "Details")
    property string primaryIcon: "info"

    /// Show the `Streams` item (jump to streams page). Defaults to
    /// true so the smart rails work out of the box; Ready to Watch
    /// turns it off because its primary already is the streams jump.
    property bool streamsVisible: true

    /// Show the `Details` item (navigate to detail page). Defaults
    /// to false; rails whose primary is *not* the detail page
    /// (Continue Watching, Ready to Watch) opt in.
    property bool detailsVisible: false

    /// Show the `Mark Watched` item. Defaults to true. Airing Soon
    /// opts out because its rows are unaired.
    property bool markWatchedVisible: true

    /// Empty label hides the destructive footer entirely.
    /// Continue Watching sets `"Remove"`; smart rails leave empty.
    property string removeLabel: ""
    property string removeIcon: "list-x"

    /// Emitted when the primary item is triggered.
    signal primaryTriggered()
    /// Emitted from `Streams`.
    signal findStreamsTriggered()
    /// Emitted from `Details` (detail-page nav) when
    /// `detailsVisible`.
    signal openDetailsTriggered()
    /// Emitted from `Mark Watched`. Series rows pop a passive
    /// notification via the consumer's VM rather than mutating
    /// anything.
    signal markWatchedTriggered()
    /// Emitted from the destructive footer item. The consumer
    /// opens its own confirm prompt and calls the matching VM
    /// slot on accept.
    signal confirmRemoveRequested()

    KinemaMenuItem {
        iconName: menu.primaryIcon
        label: menu.primaryLabel
        onTriggered: menu.primaryTriggered()
    }
    KinemaMenuItem {
        iconName: "list-video"
        label: i18nc("@action:inmenu episode rail", "Streams")
        visible: menu.streamsVisible
        onTriggered: menu.findStreamsTriggered()
    }
    KinemaMenuItem {
        iconName: "info"
        label: i18nc("@action:inmenu episode rail", "Details")
        visible: menu.detailsVisible
        onTriggered: menu.openDetailsTriggered()
    }
    QQC2.MenuSeparator {
        visible: menu.markWatchedVisible
    }
    KinemaMenuItem {
        iconName: "circle-check"
        label: i18nc("@action:inmenu episode rail", "Mark Watched")
        visible: menu.markWatchedVisible
        onTriggered: menu.markWatchedTriggered()
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "copy"
        label: i18nc("@action:inmenu episode rail", "Copy Title")
        enabled: menu.rowTitle.length > 0
        onTriggered: shell.copyToClipboard(menu.rowTitle,
            i18nc("@info:status", "Title copied to clipboard"))
    }
    KinemaMenuItem {
        iconName: "external-link"
        label: i18nc("@action:inmenu episode rail", "Open on IMDb")
        enabled: menu.rowImdbId.length > 0
        onTriggered: shell.openImdbTitle(menu.rowImdbId)
    }

    // Destructive footer. Hidden on rails that don't expose a
    // per-row remove (Up Next / Airing Soon / Recently Added are
    // computed views and the user removes via the Library grid).
    QQC2.MenuSeparator {
        visible: menu.removeLabel.length > 0
    }
    KinemaMenuItem {
        iconName: menu.removeIcon
        label: menu.removeLabel
        destructive: true
        visible: menu.removeLabel.length > 0
        onTriggered: menu.confirmRemoveRequested()
    }
}
