// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Bottom-right card that previews the next episode and counts down
 * to auto-advance. The countdown is owned by `PlaybackController`
 * (via a QTimer); we just render `playerVm.nextEpisodeCountdown`.
 */
Item {
    id: root
    property string title: ""
    property string subtitle: ""
    property int countdown: 0

    signal acceptClicked()
    signal cancelClicked()

    width: 360
    height: 130
    visible: opacity > 0
    opacity: visible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    Rectangle {
        anchors.fill: parent
        radius: Theme.radiusLg
        color: Theme.surfaceElev

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Theme.spacing
            spacing: 2

            Text {
                Layout.fillWidth: true
                text: qsTr("Up next")
                color: Theme.accent
                font.pixelSize: Theme.fontSizeSm
                font.weight: Font.Bold
            }
            Text {
                Layout.fillWidth: true
                text: root.title
                color: Theme.foreground
                font.pixelSize: Theme.fontSize
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            Text {
                Layout.fillWidth: true
                text: root.subtitle
                color: Theme.foregroundDim
                font.pixelSize: Theme.fontSizeSm
                elide: Text.ElideRight
                visible: text.length > 0
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing
                Text {
                    Layout.fillWidth: true
                    text: qsTr("In %1s").arg(root.countdown)
                    color: Theme.foregroundDim
                    font.pixelSize: Theme.fontSizeSm
                }
                Button {
                    text: qsTr("Cancel")
                    onClicked: root.cancelClicked()
                }
                Button {
                    text: qsTr("Play now")
                    highlighted: true
                    onClicked: root.acceptClicked()
                }
            }
        }
    }
}
