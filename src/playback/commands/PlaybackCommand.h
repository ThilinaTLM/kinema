// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QString>

#include <optional>
#include <variant>

namespace kinema::playback::commands {

struct PlayStream {
    domain::Stream stream;
    domain::PlaybackContext ctx;
    std::optional<domain::DownloadBackendKind> backendOverride;
};

struct PlayResolvedUrl {
    QUrl url;
    domain::PlaybackContext ctx;
};

struct SaveOffline {
    domain::Stream stream;
    domain::PlaybackContext ctx;
    std::optional<domain::DownloadBackendKind> backendOverride;
};

struct PausePlayback {
    PlaybackSessionId sessionId;
};

struct ResumePlayback {
    PlaybackSessionId sessionId;
};

struct TogglePause {
    PlaybackSessionId sessionId;
};

struct StopPlayback {
    PlaybackSessionId sessionId;
};

struct SeekRelative {
    PlaybackSessionId sessionId;
    double seconds = 0.0;
};

struct SeekAbsolute {
    PlaybackSessionId sessionId;
    double seconds = 0.0;
};

struct SetVolume {
    PlaybackSessionId sessionId;
    double percent = 100.0;
};

struct SetPlaybackRate {
    PlaybackSessionId sessionId;
    double factor = 1.0;
};

struct SelectAudioTrack {
    PlaybackSessionId sessionId;
    int trackId = -1;
};

struct SelectSubtitleTrack {
    PlaybackSessionId sessionId;
    int trackId = -1;
};

struct AttachSubtitleFile {
    PlaybackSessionId sessionId;
    QString localPath;
    QString language;
};

struct PlayNextEpisode {
    PlaybackSessionId sessionId;
};

struct PlayPreviousEpisode {
    PlaybackSessionId sessionId;
};

struct RetryTransfer {
    QString assetId;
};

struct CancelTransfer {
    QString assetId;
};

struct RemoveTransfer {
    QString assetId;
    bool removeFiles = true;
};

struct PinTransfer {
    QString assetId;
};

using PlaybackCommand = std::variant<
    PlayStream,
    PlayResolvedUrl,
    SaveOffline,
    PausePlayback,
    ResumePlayback,
    TogglePause,
    StopPlayback,
    SeekRelative,
    SeekAbsolute,
    SetVolume,
    SetPlaybackRate,
    SelectAudioTrack,
    SelectSubtitleTrack,
    AttachSubtitleFile,
    PlayNextEpisode,
    PlayPreviousEpisode,
    RetryTransfer,
    CancelTransfer,
    RemoveTransfer,
    PinTransfer>;

} // namespace kinema::playback::commands
