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

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ---- Empty states --------------------------------------
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: downloadsVm.items.count === 0
            icon.source: AppIcons.url(downloadsVm.filter === 0
                ? "download"
                : "list")
            icon.color: AppIcons.foreground
            text: switch (downloadsVm.filter) {
            case 1:
                return i18nc("@info:placeholder", "No active downloads")
            case 2:
                return i18nc("@info:placeholder", "Nothing finished yet")
            case 3:
                return i18nc("@info:placeholder", "No failed downloads")
            default:
                return i18nc("@info:placeholder", "No downloads yet")
            }
            explanation: downloadsVm.filter === 0
                ? i18nc("@info:placeholder explanation",
                    "Click the \u2b07 Download button on a stream to start "
                    + "a full background download (it keeps going whether "
                    + "you watch or not). Click \u25b6 Play to stream on "
                    + "demand \u2014 only the bytes the player needs are "
                    + "fetched, and the session quiesces when you stop "
                    + "watching.")
                : ""

            QQC2.Button {
                visible: downloadsVm.filter === 0
                Layout.alignment: Qt.AlignHCenter
                text: i18nc("@action:button browse to find streams",
                    "Browse")
                icon.source: AppIcons.url("search")
                icon.color: AppIcons.foreground
                onClicked: mainController.navigateToBrowseRequested()
            }
        }

        // ---- Downloads list ------------------------------------
        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: downloadsVm.items.count > 0
            clip: true

            ListView {
                id: list
                model: downloadsVm.items
                spacing: Kirigami.Units.smallSpacing
                topMargin: Kirigami.Units.smallSpacing
                bottomMargin: Kirigami.Units.smallSpacing
                leftMargin: Kirigami.Units.largeSpacing
                rightMargin: Kirigami.Units.largeSpacing

                delegate: DownloadRow {
                    width: list.width - list.leftMargin - list.rightMargin
                }
            }
        }
    }
}
