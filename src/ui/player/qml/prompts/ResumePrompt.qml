// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "../components"

/**
 * "Resume from X:XX" / "Start over" prompt. Centred in-scene panel
 * (no Popup) so the surrounding chrome stays visible underneath.
 */
Item {
    id: root
    property int seconds: 0
    signal resumeClicked()
    signal startOverClicked()

    width: 460
    height: 200
    visible: opacity > 0
    opacity: visible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    PopupPanel {
        anchors.fill: parent
        title: qsTr("Resume playback?")
        closable: false

        ColumnLayout {
            anchors.fill: parent
            spacing: Theme.spacingLg

            Label {
                Layout.fillWidth: true
                text: qsTr("at %1").arg(_fmt(root.seconds))
                color: Theme.foregroundDim
                horizontalAlignment: Text.AlignHCenter
                function _fmt(t) {
                    if (!isFinite(t) || t < 0) t = 0;
                    const total = Math.floor(t);
                    const h = Math.floor(total / 3600);
                    const m = Math.floor((total % 3600) / 60);
                    const s = total % 60;
                    const pad = n => (n < 10 ? "0" + n : "" + n);
                    return h > 0
                        ? h + ":" + pad(m) + ":" + pad(s)
                        : pad(m) + ":" + pad(s);
                }
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing

                Item { Layout.fillWidth: true }

                AccentButton {
                    text: qsTr("Start over")
                    onClicked: root.startOverClicked()
                }
                AccentButton {
                    text: qsTr("Resume")
                    primary: true
                    onClicked: root.resumeClicked()
                }
            }
        }
    }
}
