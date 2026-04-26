// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

/**
 * Bottom playback chrome: gradient backdrop, full-width seek bar,
 * play/pause + time on the left, volume + tracks + speed +
 * fullscreen on the right.
 *
 * Auto-hides via `chromeVisible`. Owns the seek-bar drag-to-seek
 * and emits picker-open requests upward so the modal popups remain
 * scene-level (they overlay everything else).
 *
 * The info button used to live here; it has moved to the top bar.
 * Buttons share the round-icon idiom via `IconButton.qml`.
 */
Item {
    id: root
    property var mpv: null
    property bool chromeVisible: true

    signal audioPickerRequested()
    signal subtitlePickerRequested()
    signal speedPickerRequested()
    signal fullscreenToggled()

    height: Theme.transportHeight + (mpv ? 0 : 0)

    opacity: chromeVisible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    visible: opacity > 0.001

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0.65) }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacingLg
        anchors.rightMargin: Theme.spacingLg
        anchors.bottomMargin: Theme.spacing
        spacing: 0

        // ---- Seek bar -----------------------------------------------
        SeekBar {
            id: seekBar
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.seekBarHeightHover + 8
            position: root.mpv ? root.mpv.position : 0.0
            duration: root.mpv ? root.mpv.duration : 0.0
            cacheAhead: root.mpv ? root.mpv.cacheAhead : 0.0
            chapters: playerVm.chapters
            onSeekRequested: seconds => {
                if (root.mpv) root.mpv.seekAbsolute(seconds);
            }
        }

        // ---- Button row ---------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.iconButton
            spacing: Theme.spacing

            IconButton {
                iconKind: root.mpv && root.mpv.paused ? "play" : "pause"
                onClicked: if (root.mpv) root.mpv.cyclePause()
            }

            // Time / total
            Text {
                Layout.alignment: Qt.AlignVCenter
                color: Theme.foreground
                font: Theme.monoFont
                text: {
                    const pos = root.mpv ? Math.max(0, root.mpv.position) : 0;
                    const dur = root.mpv ? Math.max(0, root.mpv.duration) : 0;
                    return _fmt(pos) + " / " + _fmt(dur);
                }
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

            Item { Layout.fillWidth: true }

            VolumeControl {
                Layout.preferredWidth: 140
                Layout.preferredHeight: Theme.iconButton
                volumePercent: root.mpv ? root.mpv.volume : 100
                muted: root.mpv ? root.mpv.muted : false
                onVolumeChanged: v => {
                    if (root.mpv) root.mpv.setVolumePercent(v);
                }
                onMuteToggled: {
                    if (root.mpv) root.mpv.setMuted(!root.mpv.muted);
                }
            }

            IconButton {
                iconKind: "audio"
                visible: playerVm.audioTracks.count > 0
                onClicked: root.audioPickerRequested()
            }
            IconButton {
                iconKind: "subtitle"
                visible: playerVm.subtitleTracks.count > 0
                onClicked: root.subtitlePickerRequested()
            }
            IconButton {
                iconKind: "speed"
                onClicked: root.speedPickerRequested()
            }
            IconButton {
                iconKind: "fullscreen"
                onClicked: root.fullscreenToggled()
            }
        }
    }
}
