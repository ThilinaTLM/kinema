// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Featured queue row. Used for the active item, or for the first
// non-played item when nothing is playing. Mirrors the
// poster-left / meta-right composition of `QueueCompactRow` but at
// a larger size, with a richer right column that includes the
// playback rail, transport controls (when the embedded player is
// driving the active item), or a Play button (when promoted from
// the pending range).
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
    property bool isActive: false
    property bool isFailed: false

    readonly property bool showLivePlayback: row.isActive
        && playQueue.embeddedPlaybackActive
    readonly property real playbackProgress:
        playQueue.playbackDurationSeconds > 0
            ? Math.max(0, Math.min(1,
                playQueue.playbackPositionSeconds
                    / playQueue.playbackDurationSeconds))
            : 0

    // ---- Outputs -------------------------------------------------
    signal playRequested()
    signal removeRequested()
    signal retryRequested()
    signal overflowRequested()

    width: ListView.view
        ? ListView.view.width - ListView.view.leftMargin
            - ListView.view.rightMargin
        : implicitWidth

    // The lead row is taller than compact rows, but only moderately
    // so the page rhythm doesn't break. Poster height drives the
    // overall layout.
    readonly property real posterHeight:
        Math.round(Kirigami.Units.gridUnit * 8)
    readonly property real posterWidth:
        Math.round(posterHeight / 1.5)

    padding: Theme.groupSpacing
    implicitHeight: Math.max(posterHeight, layout.implicitHeight)
        + padding * 2

    onClicked: if (!row.isActive) {
        row.playRequested();
    }

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

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: row.isActive
            ? Qt.alpha(Theme.accent, 0.10)
            : Kirigami.Theme.alternateBackgroundColor
        border.color: row.isActive
            ? Qt.alpha(Theme.accent, 0.40)
            : (row.isFailed
                ? Qt.alpha(Theme.negative, 0.30)
                : Qt.alpha(Theme.foreground,
                    row.hovered ? 0.18 : 0.12))
        border.width: row.isActive ? 2 : 1

        // Accent rail on the left edge \u2014 same affordance as the
        // previous design, kept thin so it reads as accent without
        // dominating the row.
        Rectangle {
            visible: row.isActive
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.margins: 1
            width: Math.max(2,
                Math.round(Kirigami.Units.smallSpacing / 2))
            radius: width / 2
            color: Theme.accent
        }
    }

    contentItem: RowLayout {
        id: layout
        spacing: Theme.groupSpacing

        // ---- Poster ---------------------------------------------
        Item {
            id: posterFrame
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: row.posterWidth
            Layout.preferredHeight: row.posterHeight

            Kirigami.ShadowedRectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Qt.alpha(Theme.foreground, 0.06)
                shadow.size: Kirigami.Units.largeSpacing
                shadow.yOffset: Kirigami.Units.smallSpacing
                shadow.color: Qt.alpha(Theme.foreground, 0.30)
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
                width: Kirigami.Units.iconSizes.large
                height: width
                source: AppIcons.url("film")
                color: Qt.alpha(Theme.foreground, 0.35)
            }
        }

        // ---- Meta column ----------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.inlineSpacing

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing

                MetaChip {
                    text: row.isActive
                        ? i18nc("@info queue badge", "Now playing")
                        : i18nc("@info queue badge", "Up next")
                    tone: "accent"
                }
                MetaChip {
                    visible: row.isFailed && !row.isActive
                    text: i18nc("@info queue badge", "Unavailable")
                    tone: "negative"
                }
                Item { Layout.fillWidth: true }
            }

            QQC2.Label {
                Layout.fillWidth: true
                text: row.title
                font.pointSize: Theme.sectionFont.pointSize
                font.weight: Font.DemiBold
                color: Theme.foreground
                elide: Text.ElideRight
                maximumLineCount: 1
                wrapMode: Text.NoWrap
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: row.subtitle.length > 0
                text: row.subtitle
                font.pointSize: Theme.defaultFont.pointSize
                color: Theme.foreground
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

            // ---- Playback rail (active only) -------------------
            Item {
                Layout.fillWidth: true
                Layout.topMargin: Theme.inlineSpacing
                visible: row.isActive
                implicitHeight: progressTrack.implicitHeight
                    + timeRow.implicitHeight + Theme.inlineSpacing

                Rectangle {
                    id: progressTrack
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: Kirigami.Units.smallSpacing + 2
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

                        Behavior on width {
                            NumberAnimation {
                                duration: Kirigami.Units.shortDuration
                            }
                        }
                    }
                }

                RowLayout {
                    id: timeRow
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: progressTrack.bottom
                    anchors.topMargin: Math.round(
                        Theme.inlineSpacing / 2)
                    visible: row.showLivePlayback

                    QQC2.Label {
                        text: row._formatTime(
                            playQueue.playbackPositionSeconds)
                        font.pointSize: Theme.captionFont.pointSize
                        color: Theme.disabled
                    }
                    Item { Layout.fillWidth: true }
                    QQC2.Label {
                        text: row._formatTime(
                            playQueue.playbackDurationSeconds)
                        font.pointSize: Theme.captionFont.pointSize
                        color: Theme.disabled
                    }
                }
            }

            // ---- Transport row ---------------------------------
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.inlineSpacing
                spacing: Theme.inlineSpacing

                QQC2.Button {
                    visible: row.showLivePlayback
                    text: playQueue.playbackPaused
                        ? i18nc("@action:button", "Resume")
                        : i18nc("@action:button", "Pause")
                    icon.source: AppIcons.url(
                        playQueue.playbackPaused ? "play"
                            : "pause")
                    icon.color: AppIcons.controlColor(enabled, highlighted)
                    display: QQC2.AbstractButton.TextBesideIcon
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
                    text: row.isActive
                        ? i18nc("@action:button", "Resume")
                        : i18nc("@action:button", "Play now")
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: row.playRequested()
                }

                QQC2.Button {
                    visible: row.isFailed && !row.isActive
                    icon.source: AppIcons.url("refresh-cw")
                    icon.color: AppIcons.controlColor(enabled, false)
                    text: i18nc("@action:button", "Retry")
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: row.retryRequested()
                }

                Item { Layout.fillWidth: true }

                QQC2.ToolButton {
                    visible: !row.isActive
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
                    text: i18nc("@action:button row actions",
                        "More actions")
                    onClicked: row.overflowRequested()
                }
            }
        }
    }
}
