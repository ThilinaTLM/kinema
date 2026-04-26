// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "../components"

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

    width: 380
    height: 150
    visible: opacity > 0
    opacity: visible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    PopupPanel {
        anchors.fill: parent
        title: qsTr("Up next")
        closable: false

        ColumnLayout {
            anchors.fill: parent
            spacing: Theme.spacingXs

            Label {
                Layout.fillWidth: true
                text: root.title
                color: Theme.foreground
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            Label {
                Layout.fillWidth: true
                text: root.subtitle
                color: Theme.foregroundDim
                font: Theme.smallFont
                elide: Text.ElideRight
                visible: text.length > 0
            }

            Item { Layout.fillHeight: true }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing
                Label {
                    Layout.fillWidth: true
                    text: qsTr("In %1s").arg(root.countdown)
                    color: Theme.foregroundDim
                    font: Theme.smallFont
                }
                AccentButton {
                    text: qsTr("Cancel")
                    onClicked: root.cancelClicked()
                }
                AccentButton {
                    text: qsTr("Play now")
                    primary: true
                    onClicked: root.acceptClicked()
                }
            }
        }
    }
}
