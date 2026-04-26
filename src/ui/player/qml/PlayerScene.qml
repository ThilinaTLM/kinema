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
 *   7. Resume + Next-episode banners        — modal-ish prompts
 *   8. Cheat sheet                          — shortcut help
 *   9. PlayerInputs                         — focus + key handling
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
        || playerVm.nextEpisodeVisible
        || playerVm.cheatSheetVisible
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
    // so they coexist with the chrome's child MouseAreas
    // (IconButton, SeekBar hover, VolumeControl thumbs, …) — every
    // mouse move / wheel / tap anywhere in the scene bumps activity,
    // even when the cursor is parked on a chrome button.
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
    TapHandler {
        acceptedButtons: Qt.AllButtons
        gesturePolicy: TapHandler.DragThreshold
        onTapped: root.bumpActivity()
        onPressedChanged: if (pressed) root.bumpActivity()
    }
    WheelHandler {
        acceptedDevices: PointerDevice.AllDevices
        onWheel: root.bumpActivity()
    }

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
        onCloseClicked: playerVm.requestClose()
    }

    TransportBar {
        id: transport
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        chromeVisible: root.chromeVisible
        mpv: mpv
        onCheatSheetRequested: playerVm.toggleCheatSheet()
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

    NextEpisodeBanner {
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacingLg
        anchors.bottom: transport.top
        anchors.bottomMargin: Theme.spacingLg
        visible: playerVm.nextEpisodeVisible
        title: playerVm.nextEpisodeTitle
        subtitle: playerVm.nextEpisodeSubtitle
        countdown: playerVm.nextEpisodeCountdown
        onAcceptClicked: playerVm.requestNextEpisodeAccept()
        onCancelClicked: playerVm.requestNextEpisodeCancel()
    }

    // ---- Layer 8: cheat sheet ---------------------------------------
    KeyboardCheatSheet {
        anchors.centerIn: parent
        visible: playerVm.cheatSheetVisible
        text: playerVm.cheatSheetText
        onCloseRequested: playerVm.setCheatSheetVisible(false)
    }

    // ---- Layer 9: input handling -------------------------------------
    PlayerInputs {
        id: inputs
        anchors.fill: parent
        mpv: mpv
        focus: true
        onActivity: root.bumpActivity()
        onTogglePauseRequested: mpv.cyclePause()
        onToggleFullscreenRequested: playerVm.requestToggleFullscreen()
        onCloseRequested: playerVm.requestClose()
        onCheatSheetRequested: playerVm.toggleCheatSheet()
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
        onPicked: id => playerVm.pickSubtitle(id)
    }

    SpeedPicker {
        id: speedPicker
        anchors.centerIn: parent
        currentSpeed: mpv.speed
        onPicked: factor => playerVm.pickSpeed(factor)
    }
}
