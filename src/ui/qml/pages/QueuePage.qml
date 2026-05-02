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

    // -------- Drag session state (mutable; owned by the page) --------
    /// Index currently being dragged. `-1` when no drag is in
    /// progress. Updated as the model reorders so the visual lift
    /// follows the row.
    property int draggedIndex: -1

    /// First index that compact pending rows can be reordered into.
    /// Played rows above the lead and the lead row itself sit
    /// outside the reorderable range.
    readonly property int firstPendingIndex: {
        if (playQueue.activeIndex >= 0) {
            return playQueue.activeIndex + 1;
        }
        const li = playQueue.leadIndex;
        return li >= 0 ? li + 1 : 0;
    }

    function _clampDropTarget(idx) {
        const count = playQueue.count;
        if (count === 0) {
            return -1;
        }
        if (idx < page.firstPendingIndex) {
            return page.firstPendingIndex;
        }
        if (idx > count - 1) {
            return count - 1;
        }
        return idx;
    }

    function _dragBegin(index, contentX, contentY) {
        if (index < page.firstPendingIndex) {
            return;
        }
        playQueue.beginReorder();
        list.interactive = false;
        page.draggedIndex = index;
    }

    function _dragMoveTo(index, contentX, contentY) {
        if (page.draggedIndex < 0) {
            return;
        }
        // Track viewport-local Y so the auto-scroll edge bands work
        // off the visible area, not the scroll content.
        const viewportY = contentY - list.contentY;
        autoScrollTimer._pointerY = viewportY;

        // Decide which model row the pointer is currently over.
        let target = list.indexAt(contentX, contentY);
        if (target < 0) {
            // Below last delegate or in an empty band; clamp to
            // the last reorderable index.
            target = playQueue.count - 1;
        }
        target = page._clampDropTarget(target);
        if (target >= 0 && target !== page.draggedIndex) {
            playQueue.moveTo(page.draggedIndex, target);
            page.draggedIndex = target;
        }

        // Toggle the auto-scroll loop based on the viewport edges.
        if (viewportY < autoScrollTimer.edgeBand
            || viewportY > list.height - autoScrollTimer.edgeBand) {
            if (!autoScrollTimer.running) {
                autoScrollTimer.start();
            }
        } else if (autoScrollTimer.running) {
            autoScrollTimer.stop();
        }
    }

    function _dragEnd() {
        autoScrollTimer.stop();
        list.interactive = true;
        playQueue.endReorder();
        page.draggedIndex = -1;
    }

    // -------- Header --------
    header: PageHeaderBar {
        title: page.title

        Item { Layout.fillWidth: true }

        MetaChip {
            visible: playQueue.count > 0
            text: i18nc("@info queue summary", "%1 total", playQueue.count)
            tone: "neutral"
        }
        MetaChip {
            visible: playQueue.playedCount > 0
            text: i18nc("@info queue summary", "%1 played",
                playQueue.playedCount)
            tone: "neutral"
        }
        MetaChip {
            visible: playQueue.count > 0
            text: playQueue.hasActiveItem
                ? i18nc("@info queue summary", "%1 next",
                    playQueue.remainingCount)
                : i18nc("@info queue summary", "%1 queued",
                    playQueue.pendingCount)
            tone: playQueue.hasActiveItem ? "accent" : "neutral"
        }
        MetaChip {
            visible: playQueue.failedCount > 0
            text: i18nc("@info queue summary", "%1 unavailable",
                playQueue.failedCount)
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
            text: i18nc("@action:inmenu", "Clear played")
            icon.source: AppIcons.url("history")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: playQueue.canClearPlayed
            onTriggered: playQueue.clearPlayed()
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Clear all except current")
            icon.source: AppIcons.url("minus")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: playQueue.canClearExceptActive
            onTriggered: clearOthersDialog.open()
        }
        QQC2.MenuSeparator { }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Clear all")
            icon.source: AppIcons.url("list-x")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: clearDialog.open()
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
            "This keeps the current item in place and removes the rest of the queue, including played items.")
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

    // -------- Auto-scroll while dragging near a viewport edge --------
    Timer {
        id: autoScrollTimer
        interval: 16
        repeat: true
        running: false

        readonly property real edgeBand: Kirigami.Units.gridUnit * 3
        readonly property real maxStep: Kirigami.Units.gridUnit
        property real _pointerY: 0

        function _scrollDelta(viewportY) {
            if (viewportY < edgeBand) {
                const ratio = 1 - (viewportY / edgeBand);
                return -Math.max(2, Math.round(maxStep * ratio));
            }
            const lower = list.height - edgeBand;
            if (viewportY > lower) {
                const ratio = (viewportY - lower) / edgeBand;
                return Math.max(2, Math.round(maxStep * ratio));
            }
            return 0;
        }

        onTriggered: {
            const delta = _scrollDelta(_pointerY);
            if (delta === 0) {
                stop();
                return;
            }
            const max = Math.max(0,
                list.contentHeight - list.height
                    + list.topMargin + list.bottomMargin);
            let next = list.contentY + delta;
            if (next < -list.topMargin) {
                next = -list.topMargin;
                stop();
            } else if (next > max) {
                next = max;
                stop();
            }
            list.contentY = next;
        }
    }

    // -------- Queue list --------
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
        cacheBuffer: Kirigami.Units.gridUnit * 12

        // Animate non-dragged rows as the dragged row passes them.
        moveDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: Kirigami.Units.shortDuration
                easing.type: Easing.OutCubic
            }
        }

        // Smooth re-layout when the active row's height changes
        // between lead and compact poses.
        addDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: Kirigami.Units.shortDuration
                easing.type: Easing.OutCubic
            }
        }
        removeDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: Kirigami.Units.shortDuration
                easing.type: Easing.OutCubic
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
            isLead: index === playQueue.leadIndex

            firstPendingIndex: page.firstPendingIndex
            draggedIndex: page.draggedIndex

            onMoveRequested: function (from, to) {
                playQueue.moveTo(from, to);
            }
            onDragStarted: function (index, cx, cy) {
                page._dragBegin(index, cx, cy);
            }
            onDragMoved: function (index, cx, cy) {
                page._dragMoveTo(index, cx, cy);
            }
            onDragEnded: function (index) {
                page._dragEnd();
            }
        }

        QQC2.ScrollBar.vertical: QQC2.ScrollBar { }

        Component.onCompleted: {
            const idx = playQueue.leadIndex;
            if (idx >= 0) {
                positionViewAtIndex(idx, ListView.Contain);
            }
        }
    }

    Connections {
        target: playQueue
        function onActiveIndexChanged() {
            if (page.draggedIndex >= 0) {
                return; // don't fight a live reorder
            }
            const idx = playQueue.leadIndex;
            if (idx >= 0 && list.visible) {
                list.positionViewAtIndex(idx, ListView.Contain);
            }
        }
    }
}
