// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2

import dev.tlmtech.kinema.app

// Overflow / right-click context menu for a `DownloadListCard` row.
//
// Follows `docs/MenuConventions.md`: primary actions at the top,
// copy / external next, destructive footer last; toggle pairs collapse
// to one stateful item (Pin / Unpin).
//
// Per the `EpisodeRailContextMenu` convention, idempotent actions call
// the `downloadsVm` / `shell` context objects directly, while the three
// actions that need a confirm prompt are surfaced as signals so the
// consuming card owns its own `Kirigami.PromptDialog`s.
KinemaMenu {
    id: menu

    required property string assetId
    required property string rowTitle
    required property string localDir
    required property string imdbId
    required property bool pinned
    required property bool canUpgrade
    required property bool complete
    required property bool hasPlayerAttached
    /// `domain::DownloadState` integer value.
    required property int state
    /// `domain::DownloadMode` integer value.
    required property int mode

    // Mirror domain enum values so the binding code carries no magic
    // numbers (kept in sync with `DownloadListCard`).
    readonly property int stateActive: 2
    readonly property int stateCompleted: 5
    readonly property int stateFailed: 6
    readonly property int stateCancelled: 7
    readonly property int modeFull: 1

    /// Full+hasPlayer pause: surfaces a confirm prompt because the
    /// attached player will buffer-starve once it catches up.
    signal pauseWhilePlayingRequested()
    /// Stop while a player is attached: surfaces a confirm prompt.
    signal stopWhilePlayingRequested()
    /// Delete cached files: surfaces a destructive confirm prompt.
    signal deleteRequested()

    KinemaMenuItem {
        iconName: "play"
        label: i18nc("@action:inmenu download row", "Play")
        visible: menu.complete
        onTriggered: downloadsVm.playDownload(menu.assetId)
    }
    KinemaMenuItem {
        iconName: "folder-open"
        label: i18nc("@action:inmenu download row", "Open Folder")
        enabled: menu.localDir.length > 0
        onTriggered: downloadsVm.openLocalDir(menu.assetId)
    }
    QQC2.MenuSeparator { }

    // Pin / Unpin collapse into one stateful item per the
    // "toggle pair = one item" convention. Pin also covers the
    // OnDemand → Full + Pinned upgrade.
    KinemaMenuItem {
        iconName: menu.pinned ? "circle-dashed" : "pin"
        label: menu.pinned
            ? i18nc("@action:inmenu download row, allow eviction",
                "Unpin")
            : i18nc("@action:inmenu download row, save (also "
                + "upgrades OnDemand to Full+Pinned)",
                "Pin")
        visible: menu.pinned
            || menu.canUpgrade
            || menu.complete
        onTriggered: {
            if (menu.pinned) {
                downloadsVm.pin(menu.assetId, false);
            } else if (menu.canUpgrade && !menu.complete) {
                downloadsVm.upgradeToFull(menu.assetId);
            } else {
                downloadsVm.pin(menu.assetId, true);
            }
        }
    }
    // Full+hasPlayer Pause lives only in the menu (the inline primary
    // slot shows "Play" so we don't offer Pause while a player is
    // attached). Confirm before pausing — playback will starve once
    // the player catches up to cached bytes.
    KinemaMenuItem {
        iconName: "pause"
        label: i18nc("@action:inmenu download row, pause this download",
            "Pause")
        visible: menu.state === menu.stateActive
            && menu.mode === menu.modeFull
            && menu.hasPlayerAttached
        onTriggered: menu.pauseWhilePlayingRequested()
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "copy"
        label: i18nc("@action:inmenu download row", "Copy Title")
        enabled: menu.rowTitle.length > 0
        onTriggered: shell.copyToClipboard(menu.rowTitle,
            i18nc("@info:status",
                "Title copied to clipboard"))
    }
    KinemaMenuItem {
        iconName: "copy"
        label: i18nc("@action:inmenu download row", "Copy Path")
        enabled: menu.localDir.length > 0
        onTriggered: shell.copyToClipboard(menu.localDir,
            i18nc("@info:status",
                "File path copied to clipboard"))
    }
    KinemaMenuItem {
        iconName: "external-link"
        label: i18nc("@action:inmenu download row",
            "Open on IMDb")
        enabled: menu.imdbId.length > 0
        onTriggered: shell.openImdbTitle(menu.imdbId)
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "x"
        label: i18nc("@action:inmenu download row, stop transfer",
            "Stop")
        destructive: true
        enabled: menu.state !== menu.stateCompleted
            && menu.state !== menu.stateFailed
            && menu.state !== menu.stateCancelled
        onTriggered: {
            if (menu.hasPlayerAttached) {
                menu.stopWhilePlayingRequested();
            } else {
                downloadsVm.cancel(menu.assetId);
            }
        }
    }
    KinemaMenuItem {
        iconName: "list-x"
        label: i18nc("@action:inmenu download row, drop the row",
            "Remove")
        destructive: true
        // Remove keeps files on disk; no confirm prompt.
        onTriggered: downloadsVm.remove(menu.assetId, false)
    }
    KinemaMenuItem {
        iconName: "trash-2"
        label: i18nc("@action:inmenu download row, drop the row "
            + "and delete cached files",
            "Delete")
        destructive: true
        onTriggered: menu.deleteRequested()
    }
}
