// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "../components"

/**
 * Subtitle picker. Includes a synthetic "Off" row at the top whose
 * selected state binds to `currentId === -1`.
 */
Popup {
    id: root
    property var model: null
    property int currentId: -1
    signal picked(int trackId)

    modal: true
    padding: 0
    // Responsive: cap at ~22 grid units, never wider than the scene
    // less standard chrome margins.
    width: Math.min(Theme.gridUnit * 22,
        parent ? parent.width - Theme.spacingLg * 2 : Theme.gridUnit * 22)
    // Extra +8 grid units vs. AudioPicker to fit the synthetic "Off"
    // row plus the two footer entries (Download / Open file).
    height: Math.min(Theme.gridUnit * 30,
        list.contentHeight + Theme.spacingLg * 8)

    background: Item {}

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.fadeMs }
            NumberAnimation { property: "scale";   from: 0.98; to: 1; duration: Theme.fadeMs }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.fadeMs }
    }

    // Emitted when the user picks the "Download subtitle…" footer
    // entry. PlayerScene wires this up to open the search sheet.
    signal downloadSubtitleRequested()
    // Emitted when the user picks the "Open subtitle file…" entry.
    signal openLocalFileRequested()

    // Bound from PlayerScene; controls the disabled state on the
    // "Download subtitle…" footer entry.
    property bool downloadEnabled: false

    contentItem: PopupPanel {
        title: qsTr("Subtitles")
        onCloseRequested: root.close()

        ColumnLayout {
            anchors.fill: parent
            spacing: 2

            // Synthetic "Off" row pinned at the top.
            PickerItem {
                Layout.fillWidth: true
                label: qsTr("Off")
                selected: root.currentId === -1
                onClicked: { root.picked(-1); root.close(); }
            }

            ListView {
                id: list
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 2
                model: root.model
                boundsBehavior: Flickable.StopAtBounds

                delegate: PickerItem {
                    width: ListView.view ? ListView.view.width : 0
                    label: forced
                        ? title + "  \u2014  " + qsTr("forced")
                        : title
                    selected: root.currentId === trackId
                    onClicked: {
                        root.picked(trackId);
                        root.close();
                    }
                }
            }

            // ---- Footer: external subtitle entries -------------------
            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: Qt.rgba(1, 1, 1, 0.08)
                Layout.topMargin: 4
                Layout.bottomMargin: 4
            }

            PickerItem {
                Layout.fillWidth: true
                label: qsTr("Download subtitle\u2026")
                enabled: root.downloadEnabled
                opacity: enabled ? 1.0 : 0.5
                ToolTip.visible: hovered && !root.downloadEnabled
                ToolTip.text: qsTr(
                    "Configure OpenSubtitles in Settings to enable")
                onClicked: {
                    root.close();
                    root.downloadSubtitleRequested();
                }
            }

            PickerItem {
                Layout.fillWidth: true
                label: qsTr("Open subtitle file\u2026")
                onClicked: {
                    root.close();
                    root.openLocalFileRequested();
                }
            }
        }
    }
}
