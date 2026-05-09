// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

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

bool isVideoFilePath(const QString& path);
bool isLikelySampleOrExtra(const QString& path, qint64 sizeBytes);
MediaFileSelection selectMediaFile(const QVector<TorrentFileEntry>& files,
    const api::PlaybackContext& ctx);

} // namespace kinema::torrent
