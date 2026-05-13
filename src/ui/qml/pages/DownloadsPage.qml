// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Downloads page — a flat, time-ordered ops console for in-flight and
// recent transfers. Cache budget / live cache usage / "Run eviction
// now" are intentionally not on this page; they live in the Settings
// "Downloads" tab (`TorrentStreamingSettingsPage.qml`).
//
// Per-title visibility ("all downloads for The Big Bang Theory") is
// surfaced on the originating Series / Movie detail page; clicking a
// row here navigates there.
//
// Chrome:
//   * Page header is the shared `PageHeaderBar` with the page title,
//     a single inline `FilterMenuButton` ("Status: …") matching the
//     pattern used on Browse / Subtitles / Streams, and at most two
//     page actions (Pause all / Resume all). The cache and the
//     "Run eviction" button live in Settings.
//   * Recent-first ordering plus eviction means there's no "Clear
//     finished" / "Clear failed" toolbar action — old rows roll off
//     naturally as new activity pushes them down and the cache budget
//     reclaims their bytes.
Kirigami.Page {
    id: page

    objectName: "downloads"
    title: downloadsVm.totalDownloadRateBps > 0
        ? i18nc("@title:window with aggregate download rate",
            "Downloads \u00b7 \u2193 %1",
            downloadsVm.totalDownloadRateText)
        : i18nc("@title:window", "Downloads")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    // ---- Page-level actions (rendered by PageHeaderBar) ----------
    // Only the bulk lifecycle controls survive at page scope; per-row
    // remove / cancel / keep-file all live in the row's overflow.
    actions: [
        Kirigami.Action {
            icon.source: AppIcons.url("pause")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button pause every active download",
                "Pause all")
            visible: downloadsVm.activeCount > 0
            onTriggered: downloadsVm.pauseAll()
        },
        Kirigami.Action {
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button resume every paused download",
                "Resume all")
            visible: downloadsVm.pausedCount > 0
            onTriggered: downloadsVm.resumeAll()
        }
    ]

    header: PageHeaderBar {
        title: page.title
        pageActions: page.actions

        // Right-align the status filter after the title.
        Item { Layout.fillWidth: true }

        // ---- Status filter --------------------------------------
        // Single-axis pick-one, mirroring the way Browse exposes Sort.
        // Hides itself on first run when there's nothing to filter.
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            visible: downloadsVm.totalCount > 0
            axisLabel: i18nc("@action:button downloads filter axis",
                "Status")
            icon.source: AppIcons.url("funnel")
            active: downloadsVm.filter !== 0
            options: [
                { value: 0,
                  label: i18nc("@item filter status", "All") },
                { value: 1,
                  label: i18nc("@item filter status", "Active") },
                { value: 2,
                  label: i18nc("@item filter status", "Completed") },
                { value: 3,
                  label: i18nc("@item filter status", "Failed") }
            ]
            currentValue: downloadsVm.filter
            onActivated: v => downloadsVm.filter = v
        }
    }

    function _emptyTitle() {
        switch (downloadsVm.filter) {
        case 1:
            return i18nc("@info:placeholder",
                "No active downloads");
        case 2:
            return i18nc("@info:placeholder",
                "Nothing finished yet");
        case 3:
            return i18nc("@info:placeholder",
                "No failed downloads");
        default:
            return i18nc("@info:placeholder", "No downloads yet");
        }
    }

    ListSurface {
        anchors.fill: parent

        state: downloadsVm.items.count > 0
            ? ListSurface.Ready
            : ListSurface.Empty
        model: downloadsVm.items

        // Horizontal margins inherit `ListSurface` defaults (0) so
        // download cards reach the page edge and their content
        // sits at the canonical `Theme.pageMargin` inset via
        // `BaseListCard`'s own padding — same as every other
        // list-style page.
        listTopMargin: Kirigami.Units.smallSpacing
        listBottomMargin: Kirigami.Units.smallSpacing
        listSpacing: Theme.listRowSpacing

        delegate: DownloadListCard {
            required property var model
            assetId: model.assetId
            title: model.title
            seriesTitle: model.seriesTitle
            episodeTitle: model.episodeTitle
            posterUrl: model.posterUrl
            state: model.state
            stateText: model.stateText
            stateTone: model.stateTone
            mode: model.mode
            modeLabel: model.modeLabel
            canUpgrade: model.canUpgrade
            canPause: model.canPause
            canResume: model.canResume
            hasPlayerAttached: model.hasPlayerAttached
            backendKind: model.backendKind
            backendIcon: model.backendIcon
            backendLabel: model.backendLabel
            pinned: model.pinned
            complete: model.complete
            progressFraction: model.progressFraction
            progressText: model.progressText
            sizeText: model.sizeText
            downloadRateText: model.downloadRateText
            peers: model.peers
            seeds: model.seeds
            etaText: model.etaText
            errorText: model.errorText
            localDir: model.localDir
            releaseName: model.releaseName
            qualityLabel: model.qualityLabel
            resolution: model.resolution
            provider: model.provider
            imdbId: model.imdbId
            kind: model.kind
            season: model.season
            episode: model.episode
        }

        // Custom empty state — the explanation copy carries a long
        // multi-paragraph hint when no filter is active, plus a
        // "Browse" call-to-action. The shared `emptyConfig` shape
        // covers both branches via `actionText`-gating.
        emptyConfig: ({
            icon: downloadsVm.filter === 0 ? "download" : "list",
            iconColor: AppIcons.foreground,
            title: page._emptyTitle(),
            explanation: downloadsVm.filter === 0
                ? i18nc("@info:placeholder explanation",
                    "Click the \u2b07 Download button on a stream to "
                    + "start a full background download (it keeps "
                    + "going whether you watch or not). Click \u25b6 "
                    + "Play to stream on demand \u2014 only the bytes "
                    + "the player needs are fetched, and the session "
                    + "quiesces when you stop watching.")
                : "",
            actionText: downloadsVm.filter === 0
                ? i18nc("@action:button browse to find streams",
                    "Browse")
                : "",
            actionIcon: "search",
            onActionTriggered: () => shell.navigateToBrowseRequested()
        })
    }
}
