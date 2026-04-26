// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * "Resume from X:XX" / "Start over" prompt. Centred dialog drawn
 * directly in-scene (no Popup) so the surrounding chrome stays
 * visible.
 */
Item {
    id: root
    // QML's `int` is 32-bit; fine for any realistic playback offset
    // (≈ 68 years at 1 s resolution).
    property int seconds: 0
    signal resumeClicked()
    signal startOverClicked()

    width: 460
    height: 180
    visible: opacity > 0
    opacity: visible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    Rectangle {
        anchors.fill: parent
        radius: Theme.radiusLg
        color: Theme.surfaceElev

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Theme.spacingLg
            spacing: Theme.spacingLg

            Text {
                Layout.fillWidth: true
                text: qsTr("Resume where you left off?")
                color: Theme.foreground
                font.pixelSize: Theme.fontSizeLg
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
            }
            Text {
                Layout.fillWidth: true
                text: qsTr("at ") + _fmt(root.seconds)
                color: Theme.foregroundDim
                font.pixelSize: Theme.fontSize
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
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing

                Item { Layout.fillWidth: true }

                Button {
                    text: qsTr("Start over")
                    onClicked: root.startOverClicked()
                }
                Button {
                    text: qsTr("Resume")
                    highlighted: true
                    onClicked: root.resumeClicked()
                }
            }
        }
    }
}
