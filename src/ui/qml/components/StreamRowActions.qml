// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Per-row action menu installed on each `StreamCard`'s "\u22ee" button.
// Play is covered by the row's primary button (and Enter / double-click /
// right-click), so the menu only carries the secondary affordances:
//   - Copy magnet / Open magnet                 : require infoHash
//   - Copy direct URL / Open direct URL         : require directUrl
//   - Play via torrent / Use torrent for this
//     download                                  : require infoHash, and
//                                                 only meaningful when
//                                                 Real-Debrid is
//                                                 configured (otherwise
//                                                 torrent is already the
//                                                 default backend) \u2014 hidden
//                                                 in that case.
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
    // Backend escape hatches. The default Play / Download buttons
    // route through Real-Debrid when configured; these leaves force
    // libtorrent for users who want to bypass RD on a specific
    // release (e.g. RD is having a slow day). Hidden when RD is not
    // configured \u2014 in that state torrent is already the default and
    // the override would be a no-op.
    QQC2.MenuItem {
        text: i18nc("@action:inmenu force libtorrent backend for play",
            "Play via torrent")
        icon.source: AppIcons.url("play")
        icon.color: AppIcons.controlColor(enabled, false)
        visible: menu.vm && menu.vm.realDebridConfigured
        enabled: menu.hasMagnet
        // 0 == api::DownloadBackendKind::Torrent
        onTriggered: menu.vm.playWithBackend(menu.row, 0)
    }
    QQC2.MenuItem {
        text: i18nc("@action:inmenu force libtorrent backend",
            "Use torrent for this download")
        icon.source: AppIcons.url("network-server-database")
        icon.color: AppIcons.controlColor(enabled, false)
        visible: menu.vm && menu.vm.realDebridConfigured
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
