// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Hero rail for the Continue Watching row at the top of Discover.
// Reads the rail model from `continueWatchingVm.model` (a
// `DiscoverSectionModel`), but uses `ProgressPosterCard` so each
// tile carries a progress bar + last-release subtitle.
//
// Activation modes:
//   - left click → resume          → `continueWatchingVm.resume(row)`
//   - right click → context menu   → choose another / remove
//
// The rail self-hides when `continueWatchingVm.empty` is true; the
// page binds `visible` accordingly.
ColumnLayout {
    id: rail
    spacing: Kirigami.Units.smallSpacing

    // ---- Header ---------------------------------------------------
    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.gridUnit
        Layout.rightMargin: Kirigami.Units.gridUnit

        Kirigami.Heading {
            level: 3
            text: continueWatchingVm.model
                ? continueWatchingVm.model.title
                : i18nc("@label discover row", "Continue Watching")
            Layout.fillWidth: true
            elide: Text.ElideRight
        }
    }

    // ---- List -----------------------------------------------------
    ListView {
        id: list
        Layout.fillWidth: true
        Layout.preferredHeight: Math.round(Theme.posterMin * 1.5)
            + Kirigami.Units.gridUnit * 3

        orientation: ListView.Horizontal
        clip: true
        spacing: Kirigami.Units.largeSpacing
        leftMargin: Kirigami.Units.gridUnit
        rightMargin: Kirigami.Units.gridUnit
        boundsBehavior: Flickable.StopAtBounds
        cacheBuffer: width
        model: continueWatchingVm.model

        delegate: ProgressPosterCard {
            width: Theme.posterMin
            height: list.height
            posterUrl:    model.posterUrl
            title:        model.title
            lastRelease:  model.lastRelease
            progress:     model.progress !== undefined ? model.progress : -1

            onClicked: continueWatchingVm.resume(index)
            onChooseAnotherRequested: continueWatchingVm.openDetail(index)
            onRemoveRequested: removeConfirm.openFor(index)
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
                icon.name: "dialog-cancel"
                onTriggered: removeConfirm.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button", "Remove")
                icon.name: "edit-delete"
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
