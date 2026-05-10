// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Per-row action menu installed on each `StreamCard`'s "\u22ee" button.
// Items are enabled based on what the row carries:
//   - Play                                      : require directUrl or infoHash
//   - Copy direct URL / Open direct URL         : require directUrl
//   - Copy magnet / Open magnet                 : require infoHash
//   - Subtitles\u2026                                  : always (stub)
//
// All actions route through the owning view-model's slots so the
// QML side never reaches into `services::StreamActions` directly.
QQC2.Menu {
    id: menu

    /// Index of the row in the streams model; passed to the VM
    /// when an action is chosen.
    property int row: -1
    /// Inputs from the row delegate. The two booleans avoid pulling
    /// the full Stream struct into QML just to gate enabled state.
    property bool hasMagnet: false
    property bool hasDirectUrl: false

    /// View-model exposing the per-row action slots.
    /// Defaults to `movieDetailVm` for the movie page; commit B's
    /// SeriesDetailPage will pass `seriesDetailVm` instead.
    property var vm: movieDetailVm

    QQC2.MenuItem {
        text: i18nc("@action:inmenu", "&Play now")
        icon.source: AppIcons.url("play")
        icon.color: AppIcons.controlColor(enabled, false)
        enabled: menu.hasDirectUrl || menu.hasMagnet
        onTriggered: menu.vm.playNow(menu.row)
    }
    QQC2.MenuSeparator { }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu", "Copy magnet link")
        icon.source: AppIcons.url("copy")
        icon.color: AppIcons.controlColor(enabled, false)
        enabled: menu.hasMagnet
        onTriggered: menu.vm.copyMagnet(menu.row)
    }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu", "Open magnet link")
        icon.source: AppIcons.url("external-link")
        icon.color: AppIcons.controlColor(enabled, false)
        enabled: menu.hasMagnet
        onTriggered: menu.vm.openMagnet(menu.row)
    }
    QQC2.MenuSeparator { }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu", "Copy direct URL")
        icon.source: AppIcons.url("copy")
        icon.color: AppIcons.controlColor(enabled, false)
        enabled: menu.hasDirectUrl
        onTriggered: menu.vm.copyDirectUrl(menu.row)
    }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu", "Open direct URL")
        icon.source: AppIcons.url("external-link")
        icon.color: AppIcons.controlColor(enabled, false)
        enabled: menu.hasDirectUrl
        onTriggered: menu.vm.openDirectUrl(menu.row)
    }
    QQC2.MenuSeparator { }
    // Backend escape hatch. The default `\u2b07 Download` button
    // routes through Real-Debrid when configured; this leaf forces
    // libtorrent for users who want to bypass RD on a specific
    // release (e.g. RD is having a slow day).
    QQC2.MenuItem {
        text: i18nc("@action:inmenu force libtorrent backend",
            "Use torrent for this download")
        icon.source: AppIcons.url("network-server-database")
        icon.color: AppIcons.controlColor(enabled, false)
        enabled: menu.hasMagnet
        // 0 == api::DownloadBackendKind::Torrent
        onTriggered: menu.vm.downloadWithBackend(menu.row, 0)
    }
    QQC2.MenuSeparator { }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu", "Subtitles\u2026")
        icon.source: AppIcons.url("captions")
        icon.color: AppIcons.controlColor(enabled, false)
        onTriggered: menu.vm.requestSubtitlesFor(menu.row)
    }
}
