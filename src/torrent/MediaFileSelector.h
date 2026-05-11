// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <QString>
#include <QVector>

#include <optional>

namespace kinema::torrent {

struct TorrentFileEntry {
    int index = -1;
    QString path;
    qint64 size = 0;
};

struct SelectedMediaFile {
    int index = -1;
    QString path;
    qint64 size = 0;
};

struct MediaFileSelection {
    std::optional<SelectedMediaFile> file;
    QString error;

    bool ok() const noexcept { return file.has_value(); }
};

struct EpisodeFileTarget {
    int season = 0;
    int episode = 0;
    SelectedMediaFile file;
};

struct EpisodePackNavigation {
    std::optional<EpisodeFileTarget> previous;
    std::optional<EpisodeFileTarget> current;
    std::optional<EpisodeFileTarget> next;

    bool navigationAvailable() const noexcept
    {
        return current.has_value()
            && (previous.has_value() || next.has_value());
    }
};

bool isVideoFilePath(const QString& path);
bool isLikelySampleOrExtra(const QString& path, qint64 sizeBytes);
MediaFileSelection selectMediaFile(const QVector<TorrentFileEntry>& files,
    const domain::PlaybackContext& ctx);
std::optional<EpisodePackNavigation> adjacentEpisodeFiles(
    const QVector<TorrentFileEntry>& files,
    int season,
    int episode);

} // namespace kinema::torrent
