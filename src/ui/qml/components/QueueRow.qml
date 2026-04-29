// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One row in the play queue list. Layout: drag handle / poster
// thumb / title + subtitle + release chips / status badge /
// overflow menu. Activates on click (Play this item now); the
// overflow menu carries Play next, Move up/down, Remove,
// Play in external player, Retry (only when Failed).
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

    width: ListView.view ? ListView.view.width : implicitWidth
    padding: Theme.groupSpacing
    implicitHeight: rowLayout.implicitHeight + padding * 2

    // Click anywhere on the row to play this item now. The
    // currently-active row routes through the queue controller's
    // `playAt(activeIndex)` which is a re-dispatch (useful for
    // restarting after a user-close).
    onClicked: playQueue.playAt(row.rowIndex)

    // Right-click to open the overflow menu directly.
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
        color: row.isActive
            ? Qt.alpha(Theme.accent, 0.10)
            : (row.hovered ? Qt.alpha(Theme.foreground, 0.04)
                           : "transparent")

        // Left accent bar marks the active row.
        Rectangle {
            visible: row.isActive
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: Math.round(Kirigami.Units.smallSpacing / 2) + 1
            color: Theme.accent
        }
    }

    contentItem: RowLayout {
        id: rowLayout
        spacing: Theme.groupSpacing

        // ---- Poster thumb ---------------------------------------
        Rectangle {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.gridUnit * 3
            Layout.preferredHeight: Math.round(
                Kirigami.Units.gridUnit * 3 * 1.5)
            radius: Theme.radius
            color: Qt.alpha(Theme.foreground, 0.08)
            clip: true

            Image {
                anchors.fill: parent
                source: row.posterUrl.length > 0
                    ? "image://kinema/queue?u=" + row.posterUrl
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                visible: row.posterUrl.length > 0
            }
        }

        // ---- Title + subtitle + release chips -------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Math.round(Theme.inlineSpacing / 2)

            QQC2.Label {
                Layout.fillWidth: true
                text: row.title
                font.pointSize: Theme.defaultFont.pointSize
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
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
                elide: Text.ElideRight
                maximumLineCount: 1
                wrapMode: Text.NoWrap
            }

            Flow {
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing
                visible: row.resolution.length > 0
                    || row.provider.length > 0

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
        }

        // ---- Status badge ---------------------------------------
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: row.isActive
                || row.status === 2 // Failed
            text: row.isActive
                ? i18nc("@info queue badge", "\u25b6 Now playing")
                : i18nc("@info queue badge", "Unavailable")
            font.pointSize: Theme.captionFont.pointSize
            font.weight: Font.DemiBold
            color: row.isActive
                ? Theme.accent
                : Theme.negative
            padding: Theme.inlineSpacing
            background: Rectangle {
                radius: height / 2
                color: row.isActive
                    ? Qt.alpha(Theme.accent, 0.12)
                    : Qt.alpha(Theme.negative, 0.12)
            }
        }

        // ---- Overflow menu --------------------------------------
        QQC2.ToolButton {
            Layout.alignment: Qt.AlignVCenter
            icon.name: "overflow-menu"
            display: QQC2.AbstractButton.IconOnly
            text: i18nc("@action:button row actions", "More actions")
            onClicked: overflowMenu.popup()
        }
    }

    // ---- Action menu --------------------------------------------
    QQC2.Menu {
        id: overflowMenu

        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "&Play now")
            icon.name: "media-playback-start"
            onTriggered: playQueue.playAt(row.rowIndex)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Retry")
            icon.name: "view-refresh"
            visible: row.status === 2 // Failed
            height: visible ? implicitHeight : 0
            onTriggered: playQueue.retryFailed(row.rowIndex)
        }
        QQC2.MenuSeparator { }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Move &up")
            icon.name: "arrow-up"
            enabled: row.rowIndex > 0
            onTriggered: playQueue.moveTo(row.rowIndex,
                row.rowIndex - 1)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Move &down")
            icon.name: "arrow-down"
            enabled: row.rowIndex >= 0
                && row.rowIndex < (playQueue.count - 1)
            onTriggered: playQueue.moveTo(row.rowIndex,
                row.rowIndex + 1)
        }
        QQC2.MenuSeparator { }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "&Remove")
            icon.name: "edit-delete"
            onTriggered: playQueue.removeAt(row.rowIndex)
        }
    }
}
