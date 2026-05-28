// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::playback::ports {

/**
 * Snapshot of the current player state — taken at construction
 * time, not signal-based. Used by projections that need a one-shot
 * view (e.g. MPRIS metadata refresh).
 */
struct PlayerSnapshot {
    bool active = false;
    bool paused = false;
    double positionSec = 0.0;
    double durationSec = 0.0;
    double volumePercent = 100.0;
    double playbackRate = 1.0;
};

/**
 * Abstract player. Concrete implementations:
 *  - `EmbeddedMpvPlayerAdapter`: wraps `ui::player::PlayerWindow`
 *    (libmpv-backed). Available only with `KINEMA_HAVE_LIBMPV`.
 *  - `ExternalPlayerAdapter`: wraps `core::PlayerLauncher` (mpv/VLC/custom).
 *
 * Concrete adapters publish player events into
 * `playback::events::PlaybackEventStream` so the session and
 * projections can react without referring to the embedded player
 * types directly.
 */
class PlayerPort
{
public:
    virtual ~PlayerPort() = default;

    /// True when the player is available for use (window attached,
    /// command resolvable, …).
    virtual bool isAvailable() const = 0;

    /// Begin playback. `resumeSeconds` is honored when the player
    /// supports it; otherwise ignored.
    virtual void play(const QUrl& url,
        const domain::PlaybackContext& ctx,
        std::optional<qint64> resumeSeconds = std::nullopt)
        = 0;

    virtual void pause() = 0;
    virtual void resume() = 0;
    virtual void togglePause() = 0;
    virtual void stop() = 0;

    virtual void seekRelative(double seconds) = 0;
    virtual void seekAbsolute(double seconds) = 0;
    virtual void setVolumePercent(double percent) = 0;
    virtual void setPlaybackRate(double factor) = 0;

    /// Select an audio / subtitle track by id. `-1` deselects.
    virtual void selectAudioTrack(int id) = 0;
    virtual void selectSubtitleTrack(int id) = 0;

    /// Attach a local subtitle file to the active session.
    /// Returns false when the player has no active stream.
    virtual bool attachSubtitleFile(const QString& localPath,
        const QString& language)
        = 0;

    virtual PlayerSnapshot snapshot() const = 0;
};

} // namespace kinema::playback::ports
