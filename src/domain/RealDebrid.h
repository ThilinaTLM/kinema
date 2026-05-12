// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDateTime>
#include <QHash>
#include <QList>
#include <QMap>
#include <QMetaType>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::domain {

/// User info returned by the Real-Debrid /user endpoint.
struct RealDebridUser {
    QString username;
    QString email;
    QString type; ///< "premium" or "free"
    std::optional<QDateTime> premiumUntil;
};

/// Result of POST /torrents/addMagnet.
struct RdAddMagnetResult {
    QString id; ///< RD torrent id
    QUrl uri; ///< self-link returned by RD
};

/// One file row from GET /torrents/info/{id}.files[].
struct RdTorrentFile {
    int id = 0; ///< 1-indexed file id used by selectFiles
    QString path; ///< "/folder/movie.mkv"
    qint64 bytes = 0;
    bool selected = false;
};

/// Result of GET /torrents/info/{id}.
struct RdTorrentInfo {
    QString id;
    QString filename;
    QString hash;
    qint64 bytes = 0;
    QString host;
    QString status; ///< "magnet_conversion", "downloading", "downloaded", "queued", ...
    int progress = 0; ///< 0..100
    QList<RdTorrentFile> files;
    /// Hoster URLs RD produced for the selected files. Order matches
    /// the ascending order of selected file ids.
    QList<QUrl> links;
};

/// Result of POST /unrestrict/link.
struct RdUnrestrictedLink {
    QString id;
    QString filename;
    qint64 fileSize = 0;
    QUrl download; ///< the actual hoster URL Kinema streams from
    QString mimeType;
    QString host;
    bool streamable = false;
};

} // namespace kinema::domain

Q_DECLARE_METATYPE(kinema::domain::RealDebridUser)
Q_DECLARE_METATYPE(kinema::domain::RdAddMagnetResult)
Q_DECLARE_METATYPE(kinema::domain::RdTorrentInfo)
Q_DECLARE_METATYPE(kinema::domain::RdUnrestrictedLink)
