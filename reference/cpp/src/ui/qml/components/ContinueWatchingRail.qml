// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Continue Watching rail for the Up Next page. Thin wrapper over
// the shared `LibraryRail` (which uses `EpisodeRailCard` cards),
// adding the right-click context menu the Continue Watching
// surface owns.
//
// Menu items (`EpisodeRailContextMenu`):
//   - Resume                          \u2192 continueWatchingVm.resume
//   - Streams                         \u2192 continueWatchingVm.openStreams
//   - Details                         \u2192 continueWatchingVm.openDetail
//   - Mark Episode Watched / Watched  \u2192 (informational stub here;
//                                       per-episode mutation lives
//                                       on the detail page)
//   - Copy Title                      \u2192 shell.copyToClipboard
//   - Open on IMDb                    \u2192 shell.openImdbTitle
//   - Remove from Continue Watching   \u2192 confirm \u2192 vm.remove
//
// Left-click on a card still resumes directly. Visual chrome
// (16:9 frame, progress bar, three meta lines) is identical to
// the smart rails \u2014 the wrapper just supplies the context menu
// and binds the model.
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
        onContextMenuRequested: (row, rowTitle, rowImdbId, kindIndex) => {
            contextMenu.targetRow = row;
            contextMenu.rowTitle = rowTitle;
            contextMenu.rowImdbId = rowImdbId;
            contextMenu.kindIndex = kindIndex;
            contextMenu.popup();
        }
    }

    EpisodeRailContextMenu {
        id: contextMenu
        primaryLabel: i18nc("@action:inmenu continue watching", "Resume")
        primaryIcon: "play"
        detailsVisible: true
        removeLabel: i18nc("@action:inmenu continue watching", "Remove")
        removeIcon: "list-x"

        onPrimaryTriggered: continueWatchingVm.resume(contextMenu.targetRow)
        onFindStreamsTriggered:
            continueWatchingVm.openStreams(contextMenu.targetRow)
        onOpenDetailsTriggered:
            continueWatchingVm.openDetail(contextMenu.targetRow)
        onMarkWatchedTriggered: {
            // Continue Watching rows are always per-episode (or
            // per-movie). The full mark-watched flow lives on the
            // detail page; surface a passive hint that routes the
            // user there rather than silently doing nothing.
            continueWatchingVm.openDetail(contextMenu.targetRow);
        }
        onConfirmRemoveRequested: removeConfirm.openFor(contextMenu.targetRow)
    }

    // Confirm dialog for the "Remove from Continue Watching" action.
    // Kirigami.PromptDialog is the modern equivalent of QMessageBox
    // and falls back gracefully on minimal styles.
    Kirigami.PromptDialog {
        id: removeConfirm

        property int targetRow: -1

        title: i18nc("@title:dialog",
            "Remove from Continue Watching?")
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
