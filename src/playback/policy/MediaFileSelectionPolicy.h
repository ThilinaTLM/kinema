// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"

#include <QString>
#include <QVector>

#include <optional>

namespace kinema::playback::policy {

struct MediaFileSelection {
    std::optional<domain::MediaFileEntry> file;
    QString error;

    bool ok() const noexcept { return file.has_value(); }
};

/// True for known video extensions (mkv, mp4, m4v, avi, mov, webm, ts).
bool isVideoFilePath(const QString& path);

/// Filter out sample, trailer, extras, etc. and obviously-too-small
/// video files (<50 MiB).
bool isLikelySampleOrExtra(const QString& path, qint64 sizeBytes);

/// Pick the best playable file for the given playback context.
MediaFileSelection selectMediaFile(
    const QVector<domain::MediaFileEntry>& files,
    const domain::PlaybackContext& ctx);

/// Build prev/current/next adjacency for a season-pack episode.
std::optional<domain::EpisodeAdjacency> adjacentEpisodeFiles(
    const QVector<domain::MediaFileEntry>& files,
    int season,
    int episode);

/// Number of entries that the selection/adjacency logic would treat
/// as playable candidates. Used for diagnostic logging.
int playableCandidateCount(const QVector<domain::MediaFileEntry>& files);

} // namespace kinema::playback::policy
