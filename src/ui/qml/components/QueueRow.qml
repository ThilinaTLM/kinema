// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

QQC2.ItemDelegate {
    id: row

    property int rowIndex: -1
    property string title: ""
    property string subtitle: ""
    property string posterUrl: ""
    property string releaseName: ""
    property string resolution: ""
    property string qualityLabel: ""
    property string provider: ""
    property var sizeBytes: -1
    /// PlayQueueViewModel.Status as int: 0=Pending, 1=Active, 2=Failed.
    property int status: 0
    property bool isActive: false

    readonly property bool isFailed: row.status === 2
    readonly property bool dragEnabled: !row.isActive
    readonly property bool showLivePlayback: row.isActive
        && playQueue.embeddedPlaybackActive
    readonly property real playbackProgress: playQueue.playbackDurationSeconds > 0
        ? Math.max(0, Math.min(1,
            playQueue.playbackPositionSeconds / playQueue.playbackDurationSeconds))
        : 0

    width: ListView.view
        ? ListView.view.width - ListView.view.leftMargin - ListView.view.rightMargin
        : implicitWidth
    padding: row.isActive ? Theme.pageMargin : Theme.groupSpacing
    implicitHeight: layout.implicitHeight + padding * 2

    function _formatTime(totalSeconds) {
        const s = Math.max(0, Math.floor(totalSeconds || 0));
        const h = Math.floor(s / 3600);
        const m = Math.floor((s % 3600) / 60);
        const sec = s % 60;
        if (h > 0) {
            return h + ":"
                + (m < 10 ? "0" : "") + m + ":"
                + (sec < 10 ? "0" : "") + sec;
        }
        return m + ":" + (sec < 10 ? "0" : "") + sec;
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

    function reorderToPointer() {
        if (!dragHandler.active || !ListView.view || row.rowIndex < 0) {
            return;
        }
        const p = dragHandle.mapToItem(ListView.view.contentItem,
            dragHandle.width / 2,
            dragHandler.centroid.position.y);
        const targetIndex = ListView.view.indexAt(p.x, p.y);
        if (targetIndex >= 0 && targetIndex !== row.rowIndex) {
            playQueue.moveTo(row.rowIndex, targetIndex);
        }
    }

    onClicked: if (!dragHandler.active) {
        playQueue.playAt(row.rowIndex);
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        propagateComposedEvents: true
        onClicked: function (mouse) {
            overflowMenu.popup();
            mouse.accepted = true;
        }
    }

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: row.isActive
            ? Qt.alpha(Theme.accent, 0.12)
            : Kirigami.Theme.alternateBackgroundColor
        border.color: row.isActive
            ? Qt.alpha(Theme.accent, 0.36)
            : (row.isFailed
                ? Qt.alpha(Theme.negative, 0.30)
                : Qt.alpha(Theme.foreground, row.hovered ? 0.12 : 0.08))
        border.width: 1

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: row.isActive
                ? Qt.alpha(Theme.accent, 0.10)
                : (row.hovered ? Qt.alpha(Theme.hover, 0.12) : "transparent")
        }

        Rectangle {
            visible: row.isActive
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: Math.max(2, Math.round(Kirigami.Units.smallSpacing / 2))
            radius: width / 2
            color: Theme.accent
        }
    }

    contentItem: ColumnLayout {
        id: layout
        spacing: row.isActive ? Theme.groupSpacing : Theme.inlineSpacing

        RowLayout {
            spacing: Theme.groupSpacing

            Item {
                id: dragHandle
                Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                visible: row.dragEnabled

                Kirigami.Icon {
                    anchors.centerIn: parent
                    width: Kirigami.Units.iconSizes.small
                    height: width
                    source: AppIcons.url("list")
                    color: row.dragEnabled
                        ? (dragHandler.active ? Theme.accent : Theme.disabled)
                        : "transparent"
                }

                DragHandler {
                    id: dragHandler
                    enabled: row.dragEnabled
                    target: null
                    xAxis.enabled: false
                    onActiveChanged: {
                        if (ListView.view) {
                            ListView.view.interactive = !active;
                        }
                    }
                    onTranslationChanged: row.reorderToPointer()
                }
            }

            Item {
                Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: row.isActive
                    ? Kirigami.Units.gridUnit * 5
                    : Kirigami.Units.gridUnit * 3.5
                Layout.preferredHeight: Math.round(width * 1.5)

                Rectangle {
                    anchors.fill: parent
                    radius: Kirigami.Units.cornerRadius
                    color: Qt.alpha(Theme.foreground, 0.08)
                    border.color: Qt.alpha(Theme.foreground, 0.08)
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

            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                spacing: Math.round(Theme.inlineSpacing / 2)

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.inlineSpacing

                    QQC2.Label {
                        Layout.fillWidth: true
                        text: row.title
                        font.pointSize: row.isActive
                            ? Theme.sectionFont.pointSize
                            : Theme.defaultFont.pointSize
                        font.weight: Font.DemiBold
                        color: Theme.foreground
                        elide: Text.ElideRight
                        maximumLineCount: 1
                        wrapMode: Text.NoWrap
                    }

                    MetaChip {
                        visible: row.isActive
                        text: i18nc("@info queue badge", "Now playing")
                        tone: "accent"
                    }
                    MetaChip {
                        visible: row.isFailed && !row.isActive
                        text: i18nc("@info queue badge", "Unavailable")
                        tone: "negative"
                    }
                }

                QQC2.Label {
                    Layout.fillWidth: true
                    visible: row.subtitle.length > 0
                    text: row.subtitle
                    font.pointSize: Theme.captionFont.pointSize
                    color: row.isActive ? Theme.foreground : Theme.disabled
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
                        tone: row.isActive ? "accent" : "neutral"
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
                        height: Theme.defaultFont.pixelSize + Theme.inlineSpacing
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
                    maximumLineCount: row.isActive ? 2 : 1
                    wrapMode: Text.NoWrap
                }
            }

            RowLayout {
                Layout.alignment: Qt.AlignTop
                spacing: Theme.inlineSpacing

                QQC2.ToolButton {
                    visible: row.isFailed && !row.isActive
                    icon.source: AppIcons.url("refresh-cw")
                    icon.color: AppIcons.controlColor(enabled, false)
                    text: i18nc("@action:button", "Retry")
                    display: QQC2.AbstractButton.IconOnly
                    onClicked: playQueue.retryFailed(row.rowIndex)
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ToolButton {
                    visible: !row.isActive
                    icon.source: AppIcons.url("trash-2")
                    icon.color: AppIcons.controlColor(enabled, false)
                    text: i18nc("@action:button", "Remove")
                    display: QQC2.AbstractButton.IconOnly
                    onClicked: playQueue.removeAt(row.rowIndex)
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ToolButton {
                    icon.source: AppIcons.url("ellipsis")
                    icon.color: AppIcons.controlColor(enabled, false)
                    display: QQC2.AbstractButton.IconOnly
                    text: i18nc("@action:button row actions", "More actions")
                    onClicked: overflowMenu.popup()
                }
            }
        }

        ColumnLayout {
            visible: row.isActive
            Layout.fillWidth: true
            spacing: Theme.inlineSpacing

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.smallSpacing + 2
                radius: height / 2
                color: Qt.alpha(Theme.foreground, 0.14)

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: parent.width * row.playbackProgress
                    radius: parent.radius
                    color: Theme.accent
                    visible: row.showLivePlayback
                }
            }

            RowLayout {
                Layout.fillWidth: true
                visible: row.showLivePlayback

                QQC2.Label {
                    text: row._formatTime(playQueue.playbackPositionSeconds)
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                }
                Item { Layout.fillWidth: true }
                QQC2.Label {
                    text: row._formatTime(playQueue.playbackDurationSeconds)
                    font.pointSize: Theme.captionFont.pointSize
                    color: Theme.disabled
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: !row.showLivePlayback
                text: i18nc("@info queue now playing helper",
                    "Live progress and playback controls appear here when using the embedded player.")
                wrapMode: Text.WordWrap
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing

                QQC2.Button {
                    visible: row.showLivePlayback
                    text: playQueue.playbackPaused
                        ? i18nc("@action:button", "Resume")
                        : i18nc("@action:button", "Pause")
                    onClicked: playQueue.togglePause()
                }
                QQC2.Button {
                    visible: row.showLivePlayback
                    enabled: row.rowIndex > 0
                    icon.source: AppIcons.url("chevron-left")
                    icon.color: AppIcons.controlColor(enabled, false)
                    text: i18nc("@action:button", "Previous")
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: playQueue.playPreviousItem()
                }
                QQC2.Button {
                    visible: row.showLivePlayback
                    enabled: row.rowIndex >= 0
                        && row.rowIndex < (playQueue.count - 1)
                    icon.source: AppIcons.url("skip-forward")
                    icon.color: AppIcons.controlColor(enabled, false)
                    text: i18nc("@action:button", "Next")
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: playQueue.playNextItem()
                }
                QQC2.Button {
                    visible: row.showLivePlayback
                    icon.source: AppIcons.url("x")
                    icon.color: AppIcons.controlColor(enabled, false)
                    text: i18nc("@action:button", "Stop")
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: playQueue.stopPlayback()
                }
                QQC2.Button {
                    visible: !row.showLivePlayback
                    highlighted: true
                    icon.source: AppIcons.url("play")
                    icon.color: AppIcons.controlColor(enabled, highlighted)
                    text: i18nc("@action:button", "Play current")
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: playQueue.playAt(row.rowIndex)
                }
                Item { Layout.fillWidth: true }
            }
        }
    }

    HoverHandler {
        id: releaseHover
    }
    QQC2.ToolTip {
        visible: releaseHover.hovered && row.releaseName.length > 0
        delay: Kirigami.Units.toolTipDelay
        text: row.releaseName
    }

    QQC2.Menu {
        id: overflowMenu

        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "&Play now")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: playQueue.playAt(row.rowIndex)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Retry")
            icon.source: AppIcons.url("refresh-cw")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: row.isFailed
            height: visible ? implicitHeight : 0
            onTriggered: playQueue.retryFailed(row.rowIndex)
        }
        QQC2.MenuSeparator {
            visible: !row.isActive
            height: visible ? implicitHeight : 0
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Move &up")
            icon.source: AppIcons.url("arrow-up")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: !row.isActive
            height: visible ? implicitHeight : 0
            enabled: row.rowIndex > 0
            onTriggered: playQueue.moveTo(row.rowIndex, row.rowIndex - 1)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Move &down")
            icon.source: AppIcons.url("arrow-down")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: !row.isActive
            height: visible ? implicitHeight : 0
            enabled: row.rowIndex >= 0
                && row.rowIndex < (playQueue.count - 1)
            onTriggered: playQueue.moveTo(row.rowIndex, row.rowIndex + 1)
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
