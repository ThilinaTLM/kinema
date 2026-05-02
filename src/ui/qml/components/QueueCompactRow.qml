// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One pending (or failed-pending) queue row. Poster on the left at
// full row height, meta column in the middle, drag handle and row
// actions on the right. Visual rhythm matches `EpisodeRow` and
// `StreamCard` so the queue page reads consistently with the rest
// of the app.
//
// Drag is press-on-handle; the actual drag session state lives on
// the queue page (lifted/drop-target visuals, insertion marker,
// auto-scroll, single-commit drop). This delegate just emits
// signals and renders its own active-drag look.
QQC2.ItemDelegate {
    id: row

    // ---- Inputs --------------------------------------------------
    property int rowIndex: -1
    property string title: ""
    property string subtitle: ""
    property string posterUrl: ""
    property string releaseName: ""
    property string resolution: ""
    property string qualityLabel: ""
    property string provider: ""
    property var sizeBytes: -1
    property bool isFailed: false
    property bool dragEnabled: true

    /// True while this delegate is the one being dragged. Driven by
    /// the queue page based on `draggedIndex`.
    property bool dragLifted: false

    // ---- Outputs -------------------------------------------------
    signal playRequested()
    signal removeRequested()
    signal retryRequested()
    signal overflowRequested()
    /// Emitted when the user begins / continues / ends a press on
    /// the drag handle. `localY` is the pointer Y relative to this
    /// row's top so the host delegate can map it into the
    /// `ListView` content coords without needing to know the
    /// drag handle's geometry.
    signal dragHandlePressed(real localY)
    signal dragHandleMoved(real localY)
    signal dragHandleReleased()

    width: ListView.view
        ? ListView.view.width - ListView.view.leftMargin
            - ListView.view.rightMargin
        : implicitWidth

    // Poster height drives the row height; everything aligns to it.
    readonly property real posterHeight:
        Math.round(Kirigami.Units.gridUnit * 5)
    readonly property real posterWidth:
        Math.round(posterHeight / 1.5)

    padding: Theme.inlineSpacing * 2
    implicitHeight: Math.max(posterHeight, layout.implicitHeight)
        + padding * 2

    onClicked: if (!row.dragLifted) {
        row.playRequested();
    }

    function _sizeText(bytesValue) {
        const bytes = Number(bytesValue);
        if (!(bytes > 0)) {
            return "";
        }
        const gib = bytes / (1024 * 1024 * 1024);
        if (gib >= 10) {
            return gib.toFixed(0) + " GB";
        }
        return gib.toFixed(1) + " GB";
    }

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: row.dragLifted
            ? Qt.alpha(Theme.accent, 0.10)
            : Kirigami.Theme.alternateBackgroundColor
        border.color: row.dragLifted
            ? Qt.alpha(Theme.accent, 0.55)
            : (row.isFailed
                ? Qt.alpha(Theme.negative, 0.30)
                : Qt.alpha(Theme.foreground,
                    row.hovered ? 0.14 : 0.08))
        border.width: row.dragLifted ? 2 : 1

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: row.dragLifted
                ? "transparent"
                : (row.hovered ? Qt.alpha(Theme.hover, 0.10)
                    : "transparent")
        }

        // Subtle layered shadow when lifted; mirrors the elevation
        // used by `KinemaArtworkFrame.shadow` on hover.
        Rectangle {
            visible: row.dragLifted
            z: -1
            anchors.fill: parent
            anchors.margins: -2
            radius: parent.radius + 2
            color: "transparent"
            border.color: Qt.alpha(Theme.accent, 0.20)
            border.width: 2
        }

        Behavior on border.color {
            ColorAnimation { duration: Kirigami.Units.shortDuration }
        }
    }

    contentItem: RowLayout {
        id: layout
        spacing: Theme.groupSpacing

        // ---- Drag handle ----------------------------------------
        Item {
            id: dragHandle
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.iconSizes.medium
            Layout.preferredHeight: row.posterHeight
            visible: row.dragEnabled

            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.small
                height: Math.round(width * 1.4)
                source: AppIcons.url("grip-vertical")
                color: row.dragLifted
                    ? Theme.accent
                    : (handleHover.hovered || handleDrag.active
                        ? Theme.foreground
                        : Theme.disabled)
                opacity: row.dragLifted || handleHover.hovered
                    || handleDrag.active ? 1.0 : 0.75

                Behavior on color {
                    ColorAnimation {
                        duration: Kirigami.Units.shortDuration
                    }
                }
            }

            HoverHandler {
                id: handleHover
                cursorShape: Qt.OpenHandCursor
            }

            DragHandler {
                id: handleDrag
                target: null
                xAxis.enabled: false
                enabled: row.dragEnabled
                onActiveChanged: {
                    if (active) {
                        const p = dragHandle.mapToItem(row,
                            0, centroid.position.y);
                        row.dragHandlePressed(p.y);
                    } else {
                        row.dragHandleReleased();
                    }
                }
                onCentroidChanged: {
                    if (!active) {
                        return;
                    }
                    const p = dragHandle.mapToItem(row,
                        0, centroid.position.y);
                    row.dragHandleMoved(p.y);
                }
            }
        }

        // ---- Poster ---------------------------------------------
        Item {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: row.posterWidth
            Layout.preferredHeight: row.posterHeight

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Qt.alpha(Theme.foreground, 0.08)
                border.color: Qt.alpha(Theme.foreground, 0.10)
                border.width: 1
            }

            Image {
                anchors.fill: parent
                anchors.margins: 1
                source: row.posterUrl.length > 0
                    ? "image://kinema/queue?u=" + row.posterUrl
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                visible: status === Image.Ready
            }

            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.medium
                height: width
                source: AppIcons.url("film")
                color: Qt.alpha(Theme.foreground, 0.35)
            }
        }

        // ---- Meta column ----------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Math.round(Theme.inlineSpacing / 2)

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing

                QQC2.Label {
                    Layout.fillWidth: true
                    text: row.title
                    font.weight: Font.DemiBold
                    color: Theme.foreground
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    wrapMode: Text.NoWrap
                }

                MetaChip {
                    visible: row.isFailed
                    text: i18nc("@info queue badge", "Unavailable")
                    tone: "negative"
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: row.subtitle.length > 0
                text: row.subtitle
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
                elide: Text.ElideRight
                maximumLineCount: 1
                wrapMode: Text.NoWrap
            }

            Flow {
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing
                visible: row.resolution.length > 0
                    || row.qualityLabel.length > 0
                    || row.provider.length > 0
                    || row._sizeText(row.sizeBytes).length > 0

                MetaChip {
                    visible: row.resolution.length > 0
                    text: row.resolution.toUpperCase()
                    tone: "neutral"
                }
                MetaChip {
                    visible: row.qualityLabel.length > 0
                        && row.qualityLabel !== row.resolution
                    text: row.qualityLabel
                    tone: "neutral"
                }
                MetaChip {
                    visible: row._sizeText(row.sizeBytes).length > 0
                    text: row._sizeText(row.sizeBytes)
                    tone: "neutral"
                }
                QQC2.Label {
                    visible: row.provider.length > 0
                    text: row.provider
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                    height: Theme.defaultFont.pixelSize
                        + Theme.inlineSpacing
                    verticalAlignment: Text.AlignVCenter
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: row.releaseName.length > 0
                text: row.releaseName
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
                elide: Text.ElideRight
                maximumLineCount: 1
                wrapMode: Text.NoWrap

                HoverHandler { id: releaseHover }
                QQC2.ToolTip.text: row.releaseName
                QQC2.ToolTip.visible: releaseHover.hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }

        // ---- Trailing actions -----------------------------------
        RowLayout {
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.inlineSpacing

            QQC2.ToolButton {
                visible: row.isFailed
                icon.source: AppIcons.url("refresh-cw")
                icon.color: AppIcons.controlColor(enabled, false)
                text: i18nc("@action:button", "Retry")
                display: QQC2.AbstractButton.IconOnly
                onClicked: row.retryRequested()
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            QQC2.ToolButton {
                icon.source: AppIcons.url("trash-2")
                icon.color: AppIcons.controlColor(enabled, false)
                text: i18nc("@action:button", "Remove")
                display: QQC2.AbstractButton.IconOnly
                onClicked: row.removeRequested()
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            QQC2.ToolButton {
                icon.source: AppIcons.url("ellipsis")
                icon.color: AppIcons.controlColor(enabled, false)
                display: QQC2.AbstractButton.IconOnly
                text: i18nc("@action:button row actions", "More actions")
                onClicked: row.overflowRequested()
            }
        }
    }
}
