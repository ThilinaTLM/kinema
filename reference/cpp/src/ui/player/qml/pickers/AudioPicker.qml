// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "../components"

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
    height: Math.min(Theme.gridUnit * 30,
        list.contentHeight + Theme.spacingLg * 6)

    // PopupPanel paints the panel chrome (radius, border, shadow,
    // header). Setting the background to a transparent Item lets it
    // show through.
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
        title: qsTr("Audio track")
        onCloseRequested: root.close()

        ListView {
            anchors.fill: parent
            id: list
            clip: true
            spacing: 2
            model: root.model
            boundsBehavior: Flickable.StopAtBounds

            delegate: PickerItem {
                width: ListView.view ? ListView.view.width : 0
                label: title
                selected: root.currentId === trackId
                onClicked: {
                    root.picked(trackId);
                    root.close();
                }
            }
        }
    }
}
