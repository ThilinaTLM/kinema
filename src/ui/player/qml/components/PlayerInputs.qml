// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Input router for the player chrome.
 *
 * This component owns the *logic* for keyboard / mouse / wheel input
 * but does not own the input handlers themselves. The handlers live
 * on the PlayerScene root, which is the item that actually gets
 * active focus (via PlayerWindow.cpp's `forceActiveFocus` on the QML
 * root). PlayerScene calls into the public functions below.
 *
 * Why not just attach `Keys.onPressed` and a scene-wide `MouseArea`
 * to this Item directly?
 *
 *   1. `Keys.onPressed` only fires on the item with active focus.
 *      `Item` is not a focus scope, so setting `focus: true` on a
 *      nested child does not propagate active focus down from the
 *      window's root focus scope to that child. The PlayerScene
 *      root *does* receive active focus, so that's where the key
 *      handler has to live.
 *
 *   2. A scene-wide `MouseArea` that wants composed clicks but also
 *      wants to let chrome buttons receive the same press is a
 *      contradiction in `MouseArea` semantics: setting
 *      `mouse.accepted = false` in `onPressed` to propagate the
 *      press also forfeits the subsequent `clicked` /
 *      `doubleClicked` for the gesture. `TapHandler` doesn't have
 *      that gotcha — it's non-grabbing by default and coexists
 *      naturally with the chrome's own `TapHandler`s on
 *      IconButton, etc.
 *
 * Bindings (wired in PlayerScene):
 *
 *   - Keyboard:
 *       Space / K           pause toggle
 *       Left  / Right       seek ±5s
 *       J     / L           seek ±10s
 *       Down  / Up          seek ±60s
 *       -     / + (or =)    volume ±5%
 *       M                   mute toggle
 *       F                   fullscreen toggle
 *       Esc                 close
 *       [     / ]           speed ±0.25x
 *   - Mouse:
 *       LMB single-click    (no-op — just bumps chrome auto-hide)
 *       LMB double-click    fullscreen toggle
 *       RMB                 pause toggle
 *       Wheel               volume ±5%
 *
 * Up/Down map to large seek instead of volume because Left/Right
 * already cover small seek and the user wanted all four arrows on
 * the timeline. Keyboard volume falls back to +/- and the wheel.
 */
QtObject {
    id: root
    property var mpv: null

    signal activity()
    signal togglePauseRequested()
    signal toggleFullscreenRequested()
    signal closeRequested()

    // ---- Public entry points (called from PlayerScene) ----------------

    function handleKey(event) {
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
            if (root.mpv) root.mpv.seekRelative(60);
            event.accepted = true;
            break;
        case Qt.Key_Down:
            if (root.mpv) root.mpv.seekRelative(-60);
            event.accepted = true;
            break;
        case Qt.Key_Plus:
        case Qt.Key_Equal:    // unshifted '+' on most layouts
            if (root.mpv) {
                root.mpv.setVolumePercent(
                    Math.min(100, root.mpv.volume + 5));
            }
            event.accepted = true;
            break;
        case Qt.Key_Minus:
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

    function handleDoubleLeftTap() {
        root.toggleFullscreenRequested();
    }

    function handleRightTap() {
        root.togglePauseRequested();
    }

    function handleWheel(angleDeltaY) {
        if (!root.mpv) return;
        const step = angleDeltaY > 0 ? 5 : -5;
        root.mpv.setVolumePercent(
            Math.max(0, Math.min(100, root.mpv.volume + step)));
        root.activity();
    }
}
