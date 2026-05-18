// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2

import dev.tlmtech.kinema.app

// Shared context menu for every Library-page rail backed by
// `EpisodeRailCard` (Continue Watching, Up Next, Airing Soon,
// Recently Added).
//
// The four rails share a common shape (Open / Streams / Mark
// Watched / Copy Title / Open on IMDb) plus a configurable
// destructive footer that Continue Watching uses for "Remove";
// everyone else leaves it empty.
//
// Consumer contract:
//
//   * Set `targetRow` before `popup()`. The signals carry no
//     args; consumers read `targetRow` to identify the row.
//   * `primaryLabel` / `primaryIcon` flip the topmost item from
//     "Open" (default) to "Resume" for Continue Watching.
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

    /// Primary-item label / icon. Continue Watching sets
    /// `Resume` + `play`; the smart rails leave the defaults.
    property string primaryLabel:
        i18nc("@action:inmenu episode rail", "Open")
    property string primaryIcon: "play"

    /// Empty label hides the destructive footer entirely.
    /// Continue Watching sets `"Remove"`; smart rails leave empty.
    property string removeLabel: ""
    property string removeIcon: "list-x"

    /// Emitted when the primary item is triggered.
    signal primaryTriggered()
    /// Emitted from `Streams`.
    signal findStreamsTriggered()
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
        onTriggered: menu.findStreamsTriggered()
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "circle-check"
        label: i18nc("@action:inmenu episode rail", "Mark Watched")
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
