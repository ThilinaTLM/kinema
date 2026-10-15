// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "../components"

/**
 * Hard-coded speed picker. Values mirror the previous Lua chrome.
 */
Popup {
    id: root
    property double currentSpeed: 1.0
    signal picked(double factor)

    modal: true
    padding: 0
    // Responsive: cap at ~18 grid units, never wider than the scene
    // less standard chrome margins.
    width: Math.min(Theme.gridUnit * 18,
        parent ? parent.width - Theme.spacingLg * 2 : Theme.gridUnit * 18)
    height: list.contentHeight + Theme.spacingLg * 6

    readonly property var values: [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]

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
        title: qsTr("Playback speed")
        onCloseRequested: root.close()

        ListView {
            id: list
            anchors.fill: parent
            clip: true
            spacing: 2
            model: root.values
            boundsBehavior: Flickable.StopAtBounds

            delegate: PickerItem {
                width: ListView.view ? ListView.view.width : 0
                label: modelData + "x"
                trailing: Math.abs(modelData - 1.0) < 0.01
                    ? qsTr("Normal") : ""
                selected: Math.abs(root.currentSpeed - modelData) < 0.01
                onClicked: { root.picked(modelData); root.close(); }
            }
        }
    }
}
