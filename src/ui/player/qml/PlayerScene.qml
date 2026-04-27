// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "components"
import "pickers"
import "prompts"

/**
 * Root composer for the player. Layered top-to-bottom:
 *
 *   1. MpvVideoItem (full-bleed)            — video pixels
 *   2. Pause-induced dim layer              — readability boost
 *   3. Top bar / Transport bar              — playback chrome
 *   4. Skip pill                            — chapter shortcuts
 *   5. Buffering / volume HUD               — transient feedback
 *   6. Center flash                         — pause/play glyph
 *   7. Resume prompt                        — modal-ish prompt
 *   8. PlayerInputs                         — focus + key handling
 *
 * `playerVm` is set as a context property by `PlayerWindow`.
 * `MpvVideoItem` lives entirely in QML so chrome can bind to its
 * properties directly; PlayerWindow looks it up after setSource via
 * its objectName.
 */
Item {
    id: root
    focus: true
    width: 1280
    height: 720

    // True while a chrome-keeping condition is active. Auto-hide
    // never wins over these.
    readonly property bool chromeForcedVisible:
        mpv.paused
        || playerVm.resumeVisible
        || mpv.buffering
        || audioPicker.opened
        || subtitlePicker.opened
        || speedPicker.opened

    // Whether the chrome bar is visible right now. Driven by the
    // single-shot hide timer below and the forced-visible flags.
    property bool chromeVisible: true

    // Last hover position seen by the scene HoverHandler. Used to
    // dedupe spurious `onPointChanged` emissions (velocity etc.)
    // so a stationary cursor still triggers auto-hide.
    property real _lastHoverX: -1
    property real _lastHoverY: -1

    // Single-shot debounced timer: each `bumpActivity()` restarts it,
    // so the chrome fades out exactly `Theme.chromeAutoHideMs` after
    // the last activity, instead of polling and giving up to 2x that
    // in worst-case latency.
    Timer {
        id: hideTimer
        interval: Theme.chromeAutoHideMs
        repeat: false
        onTriggered: {
            if (!root.chromeForcedVisible) {
                root.chromeVisible = false;
            }
        }
    }

    function bumpActivity() {
        root.chromeVisible = true;
        if (!root.chromeForcedVisible) {
            hideTimer.restart();
        } else {
            hideTimer.stop();
        }
    }

    // Whenever a forced-visible condition flips, sync the timer:
    // - flipping ON  → chrome stays up, kill the countdown.
    // - flipping OFF → resume the countdown from this moment.
    onChromeForcedVisibleChanged: {
        if (chromeForcedVisible) {
            chromeVisible = true;
            hideTimer.stop();
        } else {
            bumpActivity();
        }
    }

    // Scene-wide pointer handlers. Pointer handlers are non-grabbing,
    // so they coexist with the chrome's child MouseAreas /
    // TapHandlers (IconButton, SeekBar hover, VolumeControl thumbs,
    // …) — every mouse move / wheel / tap anywhere in the scene
    // bumps activity, even when the cursor is parked on a chrome
    // button.
    //
    // We watch `point.position` specifically (not `point` itself).
    // The `point` group also contains `velocity`, which keeps
    // updating for a moment after the cursor stops moving — if we
    // bound to `onPointChanged` we'd keep restarting the hide timer
    // forever while the cursor sits still on the window.
    HoverHandler {
        id: sceneHover
        acceptedDevices: PointerDevice.AllDevices
    }
    Connections {
        target: sceneHover
        function onPointChanged() {
            const p = sceneHover.point.position;
            if (p.x !== root._lastHoverX || p.y !== root._lastHoverY) {
                root._lastHoverX = p.x;
                root._lastHoverY = p.y;
                root.bumpActivity();
            }
        }
    }

    // RMB toggles pause; LMB double-tap toggles fullscreen. LMB
    // single-tap intentionally does nothing beyond bumping activity
    // — a stray click on the video should not pause the movie, and
    // skipping the single-tap action also lets the double-tap fire
    // immediately instead of waiting on a disambiguation timer.
    // Pause-on-keypress (Space / K) and pause-on-RMB cover the
    // "pause from the couch" need.
    //
    // `gesturePolicy: DragThreshold` keeps this handler from
    // claiming the press until the pointer moves past the drag
    // threshold, so child handlers on chrome buttons (IconButton's
    // own TapHandler) get the tap when the user clicks on them.
    TapHandler {
        id: sceneTap
        acceptedButtons: Qt.AllButtons
        gesturePolicy: TapHandler.DragThreshold
        onPressedChanged: if (pressed) root.bumpActivity()
        onTapped: (eventPoint, button) => {
            root.bumpActivity();
            if (button === Qt.RightButton) {
                inputs.handleRightTap();
            } else if (button === Qt.LeftButton
                       && sceneTap.tapCount >= 2) {
                inputs.handleDoubleLeftTap();
            }
        }
    }

    WheelHandler {
        acceptedDevices: PointerDevice.AllDevices
        onWheel: wheel => {
            root.bumpActivity();
            inputs.handleWheel(wheel.angleDelta.y);
            wheel.accepted = true;
        }
    }

    // Keyboard. PlayerWindow.cpp calls `forceActiveFocus()` on this
    // root after `show()`, so this is the item that actually has
    // active focus when no popup is open. Routing into PlayerInputs
    // keeps the binding table in one place.
    Keys.onPressed: event => inputs.handleKey(event)

    // ---- Layer 1: video ----------------------------------------------
    MpvVideoItem {
        id: mpv
        // PlayerWindow looks this up after setSource to wire the
        // C++-side observers on PlayerViewModel and to apply
        // settings; the name must stay stable.
        objectName: "kinemaMpvVideoItem"
        anchors.fill: parent
    }

    // ---- Layer 2: pause dim ------------------------------------------
    // Subtle darkening when the chrome is up so text reads cleanly
    // over bright frames.
    Rectangle {
        anchors.fill: parent
        color: "#000000"
        opacity: root.chromeVisible ? 0.18 : 0.0
        Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }
    }

    // ---- Layer 3: top bar / transport --------------------------------
    TopBar {
        id: topBar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        chromeVisible: root.chromeVisible
    }

    TransportBar {
        id: transport
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        chromeVisible: root.chromeVisible
        mpv: mpv
        onAudioPickerRequested: audioPicker.open()
        onSubtitlePickerRequested: subtitlePicker.open()
        onSpeedPickerRequested: speedPicker.open()
        onFullscreenToggled: playerVm.requestToggleFullscreen()
    }

    // ---- Layer 4: skip pill ------------------------------------------
    SkipChapterPill {
        id: skipPill
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacingLg
        anchors.bottom: transport.top
        anchors.bottomMargin: Theme.spacingLg
        visible: playerVm.skipVisible
        label: playerVm.skipLabel
        onSkipClicked: playerVm.requestSkip()
    }

    // ---- Layer 5: HUDs ------------------------------------------------
    BufferingIndicator {
        anchors.centerIn: parent
        active: mpv.buffering
        percent: mpv.bufferingPercent
    }

    VolumeHUD {
        id: volumeHud
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacingLg
        anchors.verticalCenter: parent.verticalCenter
        volumePercent: mpv.volume
        muted: mpv.muted
        // Bumped by TransportBar when the user changes volume.
    }

    Connections {
        target: mpv
        function onVolumeChanged() { volumeHud.flash(); }
        function onMuteChanged() { volumeHud.flash(); }
    }

    // ---- Layer 6: center flash ---------------------------------------
    CenterFlash {
        id: centerFlash
        anchors.centerIn: parent
        Connections {
            target: mpv
            function onPausedChanged() {
                centerFlash.flash(mpv.paused ? "pause" : "play");
            }
        }
    }

    // ---- Layer 7: prompts --------------------------------------------
    ResumePrompt {
        anchors.centerIn: parent
        visible: playerVm.resumeVisible
        seconds: playerVm.resumeSeconds
        onResumeClicked: playerVm.requestResumeAccept()
        onStartOverClicked: playerVm.requestResumeDecline()
    }

    // ---- Layer 8: input routing --------------------------------------
    // PlayerInputs is now a non-visual logic module (QtObject). The
    // actual handlers (Keys, TapHandler, WheelHandler) live on this
    // root above, because they need to be on the item that has
    // active focus / receives scene-wide pointer events.
    PlayerInputs {
        id: inputs
        mpv: mpv
        onActivity: root.bumpActivity()
        onTogglePauseRequested: mpv.cyclePause()
        onToggleFullscreenRequested: playerVm.requestToggleFullscreen()
        onCloseRequested: playerVm.requestClose()
    }

    // ---- Pickers (modal popups) --------------------------------------
    // Each Popup defaults to parenting against this scene Item; the
    // popup's own dim layer covers the video while open.
    AudioPicker {
        id: audioPicker
        anchors.centerIn: parent
        model: playerVm.audioTracks
        currentId: playerVm.currentAudioId
        onPicked: id => playerVm.pickAudio(id)
    }

    SubtitlePicker {
        id: subtitlePicker
        anchors.centerIn: parent
        model: playerVm.subtitleTracks
        currentId: playerVm.currentSubtitleId
        downloadEnabled: playerVm.subtitleDownloadEnabled
        onPicked: id => playerVm.pickSubtitle(id)
        // Hands off to MainWindow which opens the Qt Widgets
        // SubtitlesDialog parented to this player window.
        onDownloadSubtitleRequested: {
            playerVm.requestSubtitlesDialog();
        }
        onOpenLocalFileRequested: {
            playerVm.requestLocalSubtitleFile();
        }
    }

    SpeedPicker {
        id: speedPicker
        anchors.centerIn: parent
        currentSpeed: mpv.speed
        onPicked: factor => playerVm.pickSpeed(factor)
    }
}
