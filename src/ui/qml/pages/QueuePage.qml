// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Top-level Play Queue page reachable from the global drawer.
//
//   * Header carries a single "Clear queue" action when the queue
//     has rows.
//   * Body is a `ListView` of `QueueRow`s bound to the `playQueue`
//     context property exposed by `MainController`.
//   * Empty state explains how to populate the queue.
//
// Auto-advance is driven entirely by the C++ side
// (`PlayQueueController` <- `PlaybackController::endOfFile`); this
// page is just a view + per-row actions.
Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title:window", "Queue")
    objectName: "queue"

    actions: [
        Kirigami.Action {
            id: clearAction
            icon.name: "edit-clear-history"
            text: i18nc("@action:button", "Clear queue")
            visible: playQueue.count > 0
            onTriggered: clearDialog.open()
        }
    ]

    Kirigami.PromptDialog {
        id: clearDialog
        title: i18nc("@title:window confirm", "Clear queue?")
        subtitle: i18nc("@info:tooltip",
            "This removes every item, including any that's "
            + "currently playing. The active stream is stopped.")
        standardButtons: Kirigami.PromptDialog.Cancel
            | Kirigami.PromptDialog.Ok
        onAccepted: playQueue.clearAll()
    }

    // Empty state.
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4,
            Kirigami.Units.gridUnit * 28)
        visible: playQueue.empty
        icon.name: "view-media-playlist"
        text: i18nc("@info placeholder",
            "Your queue is empty")
        explanation: i18nc("@info placeholder",
            "Pick a release from any movie or episode and tap "
            + "\u201cPlay now\u201d, \u201cPlay next\u201d, or "
            + "\u201cAdd to queue\u201d.")
    }

    ListView {
        id: list
        anchors.fill: parent
        visible: !playQueue.empty
        clip: true
        spacing: 0
        model: playQueue

        // Smooth move animation when the queue is reordered.
        moveDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: Kirigami.Units.shortDuration
            }
        }

        delegate: QueueRow {
            rowIndex: index
            title: model.title
            subtitle: model.subtitle
            posterUrl: model.posterUrl
            releaseName: model.releaseName
            resolution: model.resolution
            qualityLabel: model.qualityLabel
            provider: model.provider
            sizeBytes: model.sizeBytes
            status: model.status
            isActive: model.isActive
        }
    }
}
