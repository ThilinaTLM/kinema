// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"

#include <QMetaType>
#include <QString>
#include <QUrl>
#include <QUuid>
#include <QVector>

#include <variant>

namespace kinema::playback {

/// Identity of one user-visible playback attempt.
using PlaybackSessionId = QUuid;

/// Typed reason for the natural end of a playback session.
/// Replaces stringly-typed mpv `end-file` strings at the
/// PlaybackSession boundary; adapters translate raw player
/// reasons into this enum.
enum class PlaybackEndReason {
    NaturalEof, ///< media reached EOF
    UserStop, ///< user closed the player or hit stop
    ReplacedByNewSource, ///< loadfile aborted current file
    PlayerError, ///< mpv reported an error end
    LoadTimeout, ///< load watchdog tripped
};

} // namespace kinema::playback

namespace kinema::playback::events {

// -----------------------------------------------------------------
// Event payloads
// -----------------------------------------------------------------

struct PlaybackRequested {
    PlaybackSessionId sessionId;
    domain::PlaybackContext ctx;
};

struct SourceResolving {
    PlaybackSessionId sessionId;
};

struct SourceResolved {
    PlaybackSessionId sessionId;
    domain::AssetRef asset;
};

struct TransferOpening {
    PlaybackSessionId sessionId;
    QString assetId;
};

struct TransferReady {
    PlaybackSessionId sessionId;
    QString assetId;
};

struct PlayableUrlReady {
    PlaybackSessionId sessionId;
    QString assetId;
    QUrl url;
};

struct PlayerLoading {
    PlaybackSessionId sessionId;
};

struct PlayerLoaded {
    PlaybackSessionId sessionId;
};

struct PlaybackStateChanged {
    PlaybackSessionId sessionId;
    bool playing = false;
    bool paused = false;
    bool buffering = false;
};

struct PositionTicked {
    PlaybackSessionId sessionId;
    double seconds = 0.0;
};

struct DurationChanged {
    PlaybackSessionId sessionId;
    double seconds = 0.0;
};

struct TrackListChanged {
    PlaybackSessionId sessionId;
};

struct ChapterListChanged {
    PlaybackSessionId sessionId;
};

struct BufferingChanged {
    PlaybackSessionId sessionId;
    bool buffering = false;
};

struct TransferProgressed {
    PlaybackSessionId sessionId;
    QString assetId;
    qint64 cachedBytes = 0;
    qint64 expectedBytes = -1;
};

struct TransferStatsChanged {
    PlaybackSessionId sessionId;
    QString assetId;
    qint64 bytesPerSec = 0;
    int peers = 0;
    int seeds = 0;
};

struct TransferCompleted {
    PlaybackSessionId sessionId;
    QString assetId;
};

struct TransferFailed {
    PlaybackSessionId sessionId;
    QString assetId;
    QString reason;
};

struct MoviehashComputed {
    PlaybackSessionId sessionId;
    QString hex;
};

struct SubtitleSearchStarted {
    PlaybackSessionId sessionId;
};

struct SubtitleAttached {
    PlaybackSessionId sessionId;
    QString localPath;
    QString language;
};

struct SeriesAdjacencyResolved {
    PlaybackSessionId sessionId;
    domain::EpisodeAdjacency adjacency;
};

struct CurrentStreamSizeResolved {
    PlaybackSessionId sessionId;
    qint64 sizeBytes = 0;
    QString infoHash;
};

struct PlaybackEnded {
    PlaybackSessionId sessionId;
    PlaybackEndReason reason = PlaybackEndReason::NaturalEof;
    domain::PlaybackContext ctx;
};

struct PlaybackFailed {
    PlaybackSessionId sessionId;
    QString reason;
    domain::PlaybackContext ctx;
};

using PlaybackEvent = std::variant<
    PlaybackRequested,
    SourceResolving,
    SourceResolved,
    TransferOpening,
    TransferReady,
    PlayableUrlReady,
    PlayerLoading,
    PlayerLoaded,
    PlaybackStateChanged,
    PositionTicked,
    DurationChanged,
    TrackListChanged,
    ChapterListChanged,
    BufferingChanged,
    TransferProgressed,
    TransferStatsChanged,
    TransferCompleted,
    TransferFailed,
    MoviehashComputed,
    SubtitleSearchStarted,
    SubtitleAttached,
    SeriesAdjacencyResolved,
    CurrentStreamSizeResolved,
    PlaybackEnded,
    PlaybackFailed>;

/// Extract the session id from any variant alternative.
PlaybackSessionId sessionIdOf(const PlaybackEvent& e);

} // namespace kinema::playback::events

Q_DECLARE_METATYPE(kinema::playback::PlaybackSessionId)
Q_DECLARE_METATYPE(kinema::playback::PlaybackEndReason)
Q_DECLARE_METATYPE(kinema::playback::events::PlaybackEvent)
