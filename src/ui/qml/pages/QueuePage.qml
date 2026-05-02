// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

Kirigami.Page {
    id: page

    title: i18nc("@title:window", "Queue")
    objectName: "queue"

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    header: PageHeaderBar {
        title: page.title

        Item { Layout.fillWidth: true }

        MetaChip {
            visible: playQueue.count > 0
            text: i18nc("@info queue summary", "%1 total", playQueue.count)
            tone: "neutral"
        }
        MetaChip {
            visible: playQueue.count > 0
            text: playQueue.hasActiveItem
                ? i18nc("@info queue summary", "%1 next", playQueue.remainingCount)
                : i18nc("@info queue summary", "%1 queued", playQueue.remainingCount)
            tone: playQueue.hasActiveItem ? "accent" : "neutral"
        }
        MetaChip {
            visible: playQueue.failedCount > 0
            text: i18nc("@info queue summary", "%1 unavailable", playQueue.failedCount)
            tone: "negative"
        }

        QQC2.ToolButton {
            visible: playQueue.count > 0
            icon.source: AppIcons.url("ellipsis")
            icon.color: AppIcons.controlColor(enabled, false)
            text: i18nc("@action:button", "Queue actions")
            display: QQC2.AbstractButton.IconOnly
            onClicked: queueActionsMenu.popup()
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }

    QQC2.Menu {
        id: queueActionsMenu

        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Clear all")
            icon.source: AppIcons.url("list-x")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: clearDialog.open()
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Clear all except current")
            icon.source: AppIcons.url("minus")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: playQueue.canClearExceptActive
            onTriggered: clearOthersDialog.open()
        }
    }

    Kirigami.PromptDialog {
        id: clearDialog
        title: i18nc("@title:window confirm", "Clear queue?")
        subtitle: i18nc("@info:tooltip",
            "This removes every item, including the current one. The active stream is stopped.")
        standardButtons: Kirigami.PromptDialog.Cancel
            | Kirigami.PromptDialog.Ok
        onAccepted: playQueue.clearAll()
    }

    Kirigami.PromptDialog {
        id: clearOthersDialog
        title: i18nc("@title:window confirm", "Clear everything else?")
        subtitle: i18nc("@info:tooltip",
            "This keeps the current item in place and removes the rest of the queue.")
        standardButtons: Kirigami.PromptDialog.Cancel
            | Kirigami.PromptDialog.Ok
        onAccepted: playQueue.clearAllExceptActive()
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: playQueue.empty
        icon.source: AppIcons.url("list-video")
        icon.color: AppIcons.foreground
        text: i18nc("@info placeholder", "Your queue is empty")
        explanation: i18nc("@info placeholder",
            "Pick a release from any movie or episode and tap “Play now”, “Play next”, or “Add to queue”.")
    }

    ListView {
        id: list
        anchors.fill: parent
        visible: !playQueue.empty
        clip: true
        spacing: Theme.inlineSpacing
        leftMargin: Theme.pageMargin
        rightMargin: Theme.pageMargin
        topMargin: Theme.pageTopSpacing
        bottomMargin: Theme.pageBottomSpacing
        model: playQueue

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

        QQC2.ScrollBar.vertical: QQC2.ScrollBar { }

        Component.onCompleted: {
            if (playQueue.activeIndex >= 0) {
                positionViewAtIndex(playQueue.activeIndex, ListView.Contain);
            }
        }
    }

    Connections {
        target: playQueue
        function onActiveIndexChanged() {
            if (playQueue.activeIndex >= 0 && list.visible) {
                list.positionViewAtIndex(playQueue.activeIndex,
                    ListView.Contain);
            }
        }
    }
}
