// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import dev.tlmtech.kinema.player

/**
 * Single-row playback chrome:
 *
 *   [play] [time]  ━━━━━●━━━━━━━━  [vol-icon] [vol-slider] \
 *       [audio-tracks] [subtitle] [speed] [fullscreen]
 *
 * Inlining the seek bar with the controls trades full-bar width for
 * a more compact, modern chrome. The seek bar still gets the
 * majority of the row via `Layout.fillWidth: true` and grows to a
 * thicker 14 px on hover (10 px resting); see `Theme.seekBarHeight`.
 *
 * Auto-hides via `chromeVisible`. Owns the seek-bar drag-to-seek
 * and emits picker-open requests upward so the modal popups remain
 * scene-level (they overlay everything else).
 *
 * Buttons share the round-icon idiom via `IconButton.qml`. Glyphs
 * are PathSvg-based — see `IconGlyph.qml`.
 */
Item {
    id: root
    property var mpv: null
    property bool chromeVisible: true

    signal audioPickerRequested()
    signal subtitlePickerRequested()
    signal speedPickerRequested()
    signal fullscreenToggled()

    height: Theme.transportHeight

    opacity: chromeVisible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    visible: opacity > 0.001

    // True when the player window is in OS fullscreen — used to
    // pick between the `fullscreen` (enter) and `exitFullscreen`
    // (leave) glyphs. Derived from the window's visibility because
    // PlayerViewModel does not expose an `isFullscreen` property.
    readonly property bool _isFullscreen:
        Window.window
        && Window.window.visibility === Window.FullScreen

    // Volume-aware speaker glyph. `<= 0` or `muted` → mute icon;
    // `<= 50 %` → low-volume single-wave speaker; otherwise the
    // full two-wave speaker.
    function _volumeGlyph() {
        if (!root.mpv) return "audio";
        if (root.mpv.muted || root.mpv.volume <= 0) return "mute";
        if (root.mpv.volume <= 50) return "volumeLow";
        return "audio";
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0.65) }
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacingLg
        anchors.rightMargin: Theme.spacingLg
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

        // ---- Seek bar (fills the row middle) ------------------------
        SeekBar {
            id: seekBar
            Layout.fillWidth: true
            Layout.preferredHeight:
                Theme.seekBarHeightHover + Theme.spacingSm
            Layout.alignment: Qt.AlignVCenter
            position: root.mpv ? root.mpv.position : 0.0
            duration: root.mpv ? root.mpv.duration : 0.0
            cacheAhead: root.mpv ? root.mpv.cacheAhead : 0.0
            chapters: playerVm.chapters
            onSeekRequested: seconds => {
                if (root.mpv) root.mpv.seekAbsolute(seconds);
            }
        }

        // ---- Volume cluster ----------------------------------------
        IconButton {
            iconKind: root._volumeGlyph()
            onClicked: {
                if (root.mpv) root.mpv.setMuted(!root.mpv.muted);
            }
        }

        VolumeControl {
            Layout.preferredWidth: Theme.volumeSliderWidth
            Layout.preferredHeight: Theme.iconButton
            volumePercent: root.mpv ? root.mpv.volume : 100
            muted: root.mpv ? root.mpv.muted : false
            onVolumeChanged: v => {
                if (root.mpv) root.mpv.setVolumePercent(v);
            }
        }

        // ---- Track / speed / fullscreen pickers --------------------
        IconButton {
            iconKind: "audioTracks"
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
            iconKind: root._isFullscreen ? "exitFullscreen" : "fullscreen"
            onClicked: root.fullscreenToggled()
        }
    }
}
