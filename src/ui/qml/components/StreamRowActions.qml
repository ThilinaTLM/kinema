// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2

import dev.tlmtech.kinema.app

// Per-row action menu installed on each `StreamListCard`'s "\u22ee"
// button. Play is covered by the row's primary button (and
// Enter / double-click / right-click), so the menu only carries
// secondary affordances. Wording, ordering, and ellipsis usage
// follow `docs/MenuConventions.md` \u2014 see the spec for the full
// rationale.
//
// Layout:
//
//   * Copy / Open magnet  (require `hasMagnet`)
//   * Copy / Open URL     (require `hasDirectUrl`)
//   * Copy Release Name
//   * Play / Download with Torrent Backend
//     (debrid-only escape hatches; require `hasMagnet`)
//   * Find Subtitles
//
// All actions route through the owning view-model's slots so the
// QML side never reaches into `services::StreamActions` directly.
KinemaMenu {
    id: menu

    /// Index of the row in the streams model; passed to the VM
    /// when an action is chosen.
    property int row: -1
    /// Inputs from the row delegate. The two booleans avoid pulling
    /// the full Stream struct into QML just to gate enabled state.
    property bool hasMagnet: false
    property bool hasDirectUrl: false

    /// View-model exposing the per-row action slots.
    /// `MovieDetailPage` / `SeriesDetailPage` each pass their own
    /// detail VM.
    property var vm: movieDetailVm

    KinemaMenuItem {
        iconName: "copy"
        label: i18nc("@action:inmenu", "Copy Magnet")
        enabled: menu.hasMagnet
        onTriggered: menu.vm.copyMagnet(menu.row)
    }
    KinemaMenuItem {
        iconName: "external-link"
        label: i18nc("@action:inmenu", "Open Magnet")
        enabled: menu.hasMagnet
        onTriggered: menu.vm.openMagnet(menu.row)
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "copy"
        label: i18nc("@action:inmenu", "Copy URL")
        enabled: menu.hasDirectUrl
        onTriggered: menu.vm.copyDirectUrl(menu.row)
    }
    KinemaMenuItem {
        iconName: "external-link"
        label: i18nc("@action:inmenu", "Open URL")
        enabled: menu.hasDirectUrl
        onTriggered: menu.vm.openDirectUrl(menu.row)
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "copy"
        label: i18nc("@action:inmenu", "Copy Release")
        onTriggered: menu.vm.copyReleaseName(menu.row)
    }
    QQC2.MenuSeparator { }
    // Backend escape hatches. The default Play / Download buttons
    // route through the active debrid provider when configured;
    // these leaves force libtorrent for users who want to bypass
    // the debrid provider on a specific release (e.g. it's having
    // a slow day). Hidden when no debrid provider is active \u2014 in
    // that state torrent is already the default and the override
    // would be a no-op.
    KinemaMenuItem {
        iconName: "play"
        label: i18nc("@action:inmenu force libtorrent backend for play",
            "Play via Torrent")
        visible: menu.vm && menu.vm.debridConfigured
        enabled: menu.hasMagnet
        // 0 == domain::DownloadBackendKind::Torrent
        onTriggered: menu.vm.playWithBackend(menu.row, 0)
    }
    KinemaMenuItem {
        iconName: "download"
        label: i18nc("@action:inmenu force libtorrent backend",
            "Download via Torrent")
        visible: menu.vm && menu.vm.debridConfigured
        enabled: menu.hasMagnet
        // 0 == domain::DownloadBackendKind::Torrent
        onTriggered: menu.vm.downloadWithBackend(menu.row, 0)
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "captions"
        label: i18nc("@action:inmenu", "Subtitles")
        onTriggered: menu.vm.requestSubtitlesFor(menu.row)
    }
}
