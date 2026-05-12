// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Continue Watching rail for the Up Next page. Thin wrapper over
// the shared `LibraryRail` (which uses `EpisodeRailCard` cards),
// adding the four-action right-click context menu the Continue
// Watching surface owns:
//
//   - Resume        \u2192  continueWatchingVm.resume(row)
//   - Details       \u2192  continueWatchingVm.openDetail(row)
//   - Streams       \u2192  continueWatchingVm.openStreams(row)
//   - Remove\u2026     \u2192  prompt, then continueWatchingVm.remove(row)
//
// Left-click on a card still resumes directly. Visual chrome
// (16:9 frame, progress bar, three meta lines) is identical to
// Ready to Watch / Airing Soon \u2014 the rail simply binds the
// LibraryRail to `continueWatchingVm.model` (a `LibraryRailModel`).
ColumnLayout {
    id: rail
    spacing: Theme.inlineSpacing

    LibraryRail {
        id: inner
        Layout.fillWidth: true
        title: i18nc("@title library rail", "Continue Watching")
        artworkShape: "thumbnail"
        model: continueWatchingVm.model

        onItemActivated: row => continueWatchingVm.resume(row)
        onContextMenuRequested: row => {
            contextMenu.targetRow = row;
            contextMenu.popup();
        }
    }

    QQC2.Menu {
        id: contextMenu
        property int targetRow: -1

        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Resume")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: continueWatchingVm.resume(contextMenu.targetRow)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Details")
            icon.source: AppIcons.url("info")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: continueWatchingVm.openDetail(contextMenu.targetRow)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Streams")
            icon.source: AppIcons.url("list-video")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: continueWatchingVm.openStreams(contextMenu.targetRow)
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Remove\u2026")
            icon.source: AppIcons.url("trash-2")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: removeConfirm.openFor(contextMenu.targetRow)
        }
    }

    // Confirm dialog for the "Remove from history" context action.
    // Kirigami.PromptDialog is the modern equivalent of QMessageBox
    // and falls back gracefully on minimal styles.
    Kirigami.PromptDialog {
        id: removeConfirm

        property int targetRow: -1

        title: i18nc("@title:dialog", "Remove from history?")
        subtitle: i18nc("@info",
            "This won't delete any local files. The entry will "
            + "reappear if you watch it again.")

        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Cancel")
                icon.source: AppIcons.url("x")
                icon.color: AppIcons.foreground
                onTriggered: removeConfirm.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button", "Remove")
                icon.source: AppIcons.url("trash-2")
                icon.color: AppIcons.negative
                onTriggered: {
                    if (removeConfirm.targetRow >= 0) {
                        continueWatchingVm.remove(removeConfirm.targetRow);
                    }
                    removeConfirm.close();
                }
            }
        ]

        function openFor(row) {
            targetRow = row;
            open();
        }
    }
}
