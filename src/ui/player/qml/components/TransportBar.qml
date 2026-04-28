// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import dev.tlmtech.kinema.player

/**
 * Two-row playback chrome:
 *
 *   [play] [captions] [audio-lines]              [gauge] [maximize]
 *   [12:03 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ -42:29]
 *
 * Top row hosts the action buttons (left cluster + spacer + right
 * cluster). Bottom row is the thick `SeekBar` with internal time
 * labels.
 *
 * Volume lives outside this bar entirely now — the always-visible
 * vertical volume cluster floats on the right edge of the scene
 * (`PlayerScene.qml`).
 *
 * Auto-hides via `chromeVisible`. Picker-open requests bubble up to
 * the scene as signals so the modal popups stay scene-level.
 */
Item {
    id: root
    property var mpv: null
    property bool chromeVisible: true

    signal audioPickerRequested()
    signal subtitlePickerRequested()
    signal speedPickerRequested()
    signal fullscreenToggled()

    // Two stacked rows + vertical padding. Computed dynamically so
    // a future change to icon-button or seek-bar height tracks
    // automatically. The bottom margin matches the side padding so
    // the seek bar sits visually centred in its breathing room
    // (top margin stays tight — the gradient backdrop already
    // softens that edge).
    implicitHeight:
        Theme.iconButton + Theme.seekBarHeightThick
        + Theme.spacingSm * 3 + Theme.spacingLg

    opacity: chromeVisible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    visible: opacity > 0.001

    // True when the player window is in OS fullscreen — used to
    // pick between the `maximize` (enter) and `minimize` (leave)
    // glyphs. Derived from the window's visibility because
    // PlayerViewModel does not expose an `isFullscreen` property.
    readonly property bool _isFullscreen:
        Window.window
        && Window.window.visibility === Window.FullScreen

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0.65) }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        // Side padding aligns with the top-bar's title indent
        // (`TopBar` uses `Theme.spacingLg`) and with the volume
        // cluster's right margin in `PlayerScene` so the chrome's
        // outer rhythm is uniform on all four edges of the video.
        anchors.leftMargin:   Theme.spacingLg
        anchors.rightMargin:  Theme.spacingLg
        anchors.topMargin:    Theme.spacingSm
        // Match the side padding so the seek bar has the same
        // breathing room below it as it does to its left and right.
        anchors.bottomMargin: Theme.spacingLg
        spacing: Theme.spacingSm

        // ---- Top row: action buttons ------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing

            IconButton {
                iconKind: root.mpv && root.mpv.paused ? "play" : "pause"
                onClicked: if (root.mpv) root.mpv.cyclePause()
            }
            IconButton {
                iconKind: "captions"
                visible: playerVm.subtitleTracks.count > 0
                onClicked: root.subtitlePickerRequested()
            }
            IconButton {
                iconKind: "audioLines"
                visible: playerVm.audioTracks.count > 0
                onClicked: root.audioPickerRequested()
            }

            Item { Layout.fillWidth: true }

            IconButton {
                iconKind: "gauge"
                onClicked: root.speedPickerRequested()
            }
            IconButton {
                iconKind: root._isFullscreen ? "minimize" : "maximize"
                onClicked: root.fullscreenToggled()
            }
        }

        // ---- Bottom row: thick seek bar --------------------------
        SeekBar {
            id: seekBar
            Layout.fillWidth: true
            Layout.preferredHeight:
                Theme.seekBarHeightThick + Theme.spacingSm
            position:   root.mpv ? root.mpv.position   : 0.0
            duration:   root.mpv ? root.mpv.duration   : 0.0
            cacheAhead: root.mpv ? root.mpv.cacheAhead : 0.0
            chapters:   playerVm.chapters
            onSeekRequested: seconds => {
                if (root.mpv) root.mpv.seekAbsolute(seconds);
            }
        }
    }
}
