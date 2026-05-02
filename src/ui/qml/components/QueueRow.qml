// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Coordinator delegate for the queue list. Picks one of three
// variants per row:
//
//   * `QueuePlayedRow`   when the row's status is `Played` (3)
//   * `QueueLeadRow`     when the row is the active item, or the
//                        first non-played row while idle
//   * `QueueCompactRow`  otherwise (pending, failed-pending)
//
// Drag state lives on the host `QueuePage`; this delegate simply
// surfaces drag affordances on compact rows and emits drag
// signals upward via the loaded variant.
//
// All routing of action signals lands in the same overflow menu
// here so each variant stays presentational.
Item {
    id: row

    // ---- Inputs from the model -----------------------------------
    property int rowIndex: -1
    property string title: ""
    property string subtitle: ""
    property string posterUrl: ""
    property string releaseName: ""
    property string resolution: ""
    property string qualityLabel: ""
    property string provider: ""
    property var sizeBytes: -1
    /// PlayQueueViewModel.Status as int: 0=Pending, 1=Active,
    /// 2=Failed, 3=Played.
    property int status: 0
    property bool isActive: false

    /// Driven by the queue page based on `playQueue.leadIndex`.
    property bool isLead: false

    // ---- Drag state injected by the queue page -------------------
    property int draggedIndex: -1
    /// True when the user is currently mid-drag (any row).
    readonly property bool dragActive: row.draggedIndex >= 0
    /// True when *this* delegate is the one being dragged.
    readonly property bool dragLifted: row.draggedIndex === row.rowIndex
    /// First index in the pending range (rows that can be reordered).
    /// Compact rows above this are played and not draggable; the
    /// active row is also not draggable. The page passes this in so
    /// each delegate doesn't have to recompute it.
    property int firstPendingIndex: 0

    // ---- Outputs to the queue page -------------------------------
    signal moveRequested(int from, int to)
    /// Drag-handle events. `contentY` is the pointer Y in the
    /// `ListView`'s content coords (delegate `y` + local pointer
    /// Y), suitable for `ListView.indexAt(...)` directly. `contentX`
    /// is fixed to the list's horizontal centre so `indexAt` always
    /// falls inside the row's horizontal range.
    signal dragStarted(int index, real contentX, real contentY)
    signal dragMoved(int index, real contentX, real contentY)
    signal dragEnded(int index)

    width: ListView.view
        ? ListView.view.width
            - ListView.view.leftMargin
            - ListView.view.rightMargin
        : 0

    // ---- Variant resolution -------------------------------------
    readonly property bool isPlayed: row.status === 3
    readonly property bool useLead: row.isActive
        || (row.isLead && !row.isPlayed)
    readonly property bool useCompact:
        !row.useLead && !row.isPlayed

    readonly property bool dragEnabled:
        row.useCompact && row.rowIndex >= row.firstPendingIndex

    implicitHeight: variantLoader.item
        ? variantLoader.item.implicitHeight
        : 0

    // ---- Right-click anywhere on the row pops the overflow menu --
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        propagateComposedEvents: true
        onClicked: function (mouse) {
            overflowMenu.popup();
            mouse.accepted = true;
        }
    }

    // ---- Variant ------------------------------------------------
    Loader {
        id: variantLoader
        anchors.fill: parent
        sourceComponent: row.useLead
            ? leadComponent
            : (row.isPlayed ? playedComponent : compactComponent)
    }

    Component {
        id: leadComponent
        QueueLeadRow {
            rowIndex: row.rowIndex
            title: row.title
            subtitle: row.subtitle
            posterUrl: row.posterUrl
            releaseName: row.releaseName
            resolution: row.resolution
            qualityLabel: row.qualityLabel
            provider: row.provider
            sizeBytes: row.sizeBytes
            isActive: row.isActive
            isFailed: row.status === 2
            onPlayRequested: playQueue.playAt(row.rowIndex)
            onRemoveRequested: playQueue.removeAt(row.rowIndex)
            onRetryRequested: playQueue.retryFailed(row.rowIndex)
            onOverflowRequested: overflowMenu.popup()
        }
    }

    Component {
        id: compactComponent
        QueueCompactRow {
            rowIndex: row.rowIndex
            title: row.title
            subtitle: row.subtitle
            posterUrl: row.posterUrl
            releaseName: row.releaseName
            resolution: row.resolution
            qualityLabel: row.qualityLabel
            provider: row.provider
            sizeBytes: row.sizeBytes
            isFailed: row.status === 2
            dragEnabled: row.dragEnabled
            dragLifted: row.dragLifted
            opacity: row.dragActive && !row.dragLifted ? 0.92 : 1.0
            Behavior on opacity {
                NumberAnimation {
                    duration: Kirigami.Units.shortDuration
                }
            }
            onPlayRequested: playQueue.playAt(row.rowIndex)
            onRemoveRequested: playQueue.removeAt(row.rowIndex)
            onRetryRequested: playQueue.retryFailed(row.rowIndex)
            onOverflowRequested: overflowMenu.popup()
            onDragHandlePressed: function (localY) {
                const view = row.ListView.view;
                const cx = view ? view.width / 2 : 0;
                row.dragStarted(row.rowIndex, cx, row.y + localY);
            }
            onDragHandleMoved: function (localY) {
                const view = row.ListView.view;
                const cx = view ? view.width / 2 : 0;
                row.dragMoved(row.rowIndex, cx, row.y + localY);
            }
            onDragHandleReleased: row.dragEnded(row.rowIndex)
        }
    }

    Component {
        id: playedComponent
        QueuePlayedRow {
            rowIndex: row.rowIndex
            title: row.title
            subtitle: row.subtitle
            posterUrl: row.posterUrl
            onReplayRequested: playQueue.playAt(row.rowIndex)
            onRemoveRequested: playQueue.removeAt(row.rowIndex)
            onOverflowRequested: overflowMenu.popup()
        }
    }

    // ---- Shared overflow menu -----------------------------------
    QQC2.Menu {
        id: overflowMenu

        QQC2.MenuItem {
            text: row.isPlayed
                ? i18nc("@action:inmenu replay finished item", "&Replay")
                : i18nc("@action:inmenu", "&Play now")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: playQueue.playAt(row.rowIndex)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Retry")
            icon.source: AppIcons.url("refresh-cw")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: row.status === 2
            height: visible ? implicitHeight : 0
            onTriggered: playQueue.retryFailed(row.rowIndex)
        }
        QQC2.MenuSeparator {
            visible: row.dragEnabled
            height: visible ? implicitHeight : 0
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Move &up")
            icon.source: AppIcons.url("arrow-up")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: row.dragEnabled
            height: visible ? implicitHeight : 0
            enabled: row.rowIndex > row.firstPendingIndex
            onTriggered: row.moveRequested(row.rowIndex,
                row.rowIndex - 1)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Move &down")
            icon.source: AppIcons.url("arrow-down")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: row.dragEnabled
            height: visible ? implicitHeight : 0
            enabled: row.rowIndex >= 0
                && row.rowIndex < (playQueue.count - 1)
            onTriggered: row.moveRequested(row.rowIndex,
                row.rowIndex + 1)
        }
        QQC2.MenuSeparator { }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "&Remove")
            icon.source: AppIcons.url("trash-2")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: playQueue.removeAt(row.rowIndex)
        }
    }
}
