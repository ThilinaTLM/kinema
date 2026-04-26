// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Central key + click + wheel handler.
 *
 * Hover / move tracking for the chrome auto-hide timer is handled by
 * scene-level pointer handlers in PlayerScene.qml — those don't grab
 * events, so they coexist with the chrome's own MouseAreas. This
 * component owns:
 *
 *   - Keyboard shortcuts (space/k pause, j/l seek, arrows, m, f, ?)
 *   - Single-click → toggle pause, double-click → toggle fullscreen
 *   - Wheel → volume
 *
 * Most keys translate directly to MpvVideoItem methods; a few
 * window-level actions (close, fullscreen, cheat sheet) bubble out
 * via signals so the scene composer can route them.
 */
Item {
    id: root
    property var mpv: null

    signal activity()
    signal togglePauseRequested()
    signal toggleFullscreenRequested()
    signal closeRequested()
    signal cheatSheetRequested()

    focus: true

    // Mouse activity → bump chrome.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        hoverEnabled: true
        propagateComposedEvents: true

        onPressed: mouse => {
            root.activity();
            mouse.accepted = false; // let other items see clicks
        }
        onClicked: mouse => {
            // Single-click toggles pause; double-click toggles fullscreen.
            if (mouse.button === Qt.LeftButton) {
                clickTimer.restart();
            }
            mouse.accepted = false;
        }
        onDoubleClicked: mouse => {
            clickTimer.stop();
            if (mouse.button === Qt.LeftButton) {
                root.toggleFullscreenRequested();
            }
            mouse.accepted = true;
        }
        onWheel: wheel => {
            const step = wheel.angleDelta.y > 0 ? 5 : -5;
            if (root.mpv) {
                root.mpv.setVolumePercent(
                    Math.max(0,
                        Math.min(100, root.mpv.volume + step)));
            }
            root.activity();
            wheel.accepted = true;
        }

        Timer {
            id: clickTimer
            interval: 220
            onTriggered: root.togglePauseRequested()
        }
    }

    // Key handling. We attach to the root Item so any focused child
    // (no popup open) sees these.
    Keys.onPressed: event => {
        root.activity();
        switch (event.key) {
        case Qt.Key_Space:
        case Qt.Key_K:
            root.togglePauseRequested();
            event.accepted = true;
            break;
        case Qt.Key_Left:
            if (root.mpv) root.mpv.seekRelative(-5);
            event.accepted = true;
            break;
        case Qt.Key_Right:
            if (root.mpv) root.mpv.seekRelative(5);
            event.accepted = true;
            break;
        case Qt.Key_J:
            if (root.mpv) root.mpv.seekRelative(-10);
            event.accepted = true;
            break;
        case Qt.Key_L:
            if (root.mpv) root.mpv.seekRelative(10);
            event.accepted = true;
            break;
        case Qt.Key_Up:
            if (root.mpv) {
                root.mpv.setVolumePercent(
                    Math.min(100, root.mpv.volume + 5));
            }
            event.accepted = true;
            break;
        case Qt.Key_Down:
            if (root.mpv) {
                root.mpv.setVolumePercent(
                    Math.max(0, root.mpv.volume - 5));
            }
            event.accepted = true;
            break;
        case Qt.Key_M:
            if (root.mpv) root.mpv.setMuted(!root.mpv.muted);
            event.accepted = true;
            break;
        case Qt.Key_F:
            root.toggleFullscreenRequested();
            event.accepted = true;
            break;
        case Qt.Key_Escape:
            root.closeRequested();
            event.accepted = true;
            break;
        case Qt.Key_Question:
        case Qt.Key_Slash: // some layouts emit / for ?
            if (event.modifiers & Qt.ShiftModifier
                || event.key === Qt.Key_Question) {
                root.cheatSheetRequested();
                event.accepted = true;
            }
            break;
        case Qt.Key_BracketLeft:
            if (root.mpv) {
                root.mpv.setSpeed(Math.max(0.25, root.mpv.speed - 0.25));
            }
            event.accepted = true;
            break;
        case Qt.Key_BracketRight:
            if (root.mpv) {
                root.mpv.setSpeed(Math.min(4.0, root.mpv.speed + 0.25));
            }
            event.accepted = true;
            break;
        }
    }
}
