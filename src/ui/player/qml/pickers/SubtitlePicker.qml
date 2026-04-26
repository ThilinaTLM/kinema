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
    width: 360
    height: Math.min(480, list.contentHeight + 132)

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
        }
    }
}
