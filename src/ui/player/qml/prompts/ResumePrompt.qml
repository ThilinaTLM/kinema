// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import dev.tlmtech.kinema.player

import "../components"

/**
 * "Resume from X:XX" / "Start over" prompt. Centred in-scene panel
 * (no Popup) so the surrounding chrome stays visible underneath.
 *
 * The body is built from Kirigami primitives + stock QQC2 controls
 * so it picks up Plasma typography and Breeze button styling. The
 * outer `PopupPanel` keeps the dark Header-colorSet surface so the
 * prompt remains readable over arbitrary video frames — see the
 * top comment of `PopupPanel.qml` for the rationale on why we don't
 * use `Kirigami.PromptDialog` here (its View-colorSet hardcode would
 * produce a light prompt on light Plasma schemes).
 */
Item {
    id: root
    property int seconds: 0
    signal resumeClicked()
    signal startOverClicked()

    // Responsive: cap at ~24 grid units, never wider than the parent
    // less standard chrome margins. Height tracks the actual content
    // (header + timestamp + button row + panel margins) instead of
    // padding to a fixed 13 grid units — the prompt's body is just a
    // single line of dim text, so a tall panel reads as broken.
    width: Math.min(Theme.gridUnit * 24,
        parent ? parent.width - Theme.spacingLg * 2 : Theme.gridUnit * 24)
    height: Math.min(Theme.gridUnit * 8,
        parent ? parent.height - Theme.spacingLg * 2 : Theme.gridUnit * 8)
    visible: opacity > 0
    opacity: visible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    PopupPanel {
        anchors.fill: parent
        title: qsTr("Resume playback?")
        closable: false

        ColumnLayout {
            anchors.fill: parent
            spacing: Theme.spacing

            QQC2.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("at %1").arg(_fmt(root.seconds))
                color: Kirigami.Theme.disabledTextColor
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

            // Footer buttons sit immediately under the timestamp — no
            // expanding spacer above them, so the panel hugs its
            // content tightly. Stock QQC2.Button renders through
            // org.kde.desktop → Breeze; the Header colorSet inherited
            // from PopupPanel paints them against the dark titlebar
            // palette, and `highlighted: true` picks
            // Kirigami.Theme.highlightColor for the primary action.
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Item { Layout.fillWidth: true }

                QQC2.Button {
                    text: qsTr("Start over")
                    onClicked: root.startOverClicked()
                }
                QQC2.Button {
                    text: qsTr("Resume")
                    highlighted: true
                    onClicked: root.resumeClicked()
                }
            }
        }
    }
}
