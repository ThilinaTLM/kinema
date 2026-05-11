// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDateTime>
#include <QList>
#include <QMetaType>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::domain {

/// User info returned by AllDebrid's GET /v4/user endpoint.
/// Trial users surface as `isPremium=true && isTrial=true` upstream;
/// we collapse that into the `premium` flag and only block magnet
/// upload at resolution time when the API says `MUST_BE_PREMIUM`.
struct AllDebridUser {
    QString username;
    QString email;
    bool isPremium = false;
    bool isTrial = false;
    std::optional<QDateTime> premiumUntil;
};

/// One row from POST /v4/magnet/upload's `magnets[]` array.
struct AdAddMagnetResult {
    qint64 id = 0; ///< AllDebrid magnet id (numeric)
    QString hash; ///< lower-case hex
    QString name; ///< filename or 'noname'
    qint64 sizeBytes = 0;
    bool ready = false; ///< true when AllDebrid already has the bytes
};

/// Subset of POST /v4.1/magnet/status used by the resolver. We don't
/// surface the live-mode delta fields here \u2014 the resolver polls in
/// classic mode.
struct AdMagnetStatus {
    qint64 id = 0;
    QString filename;
    qint64 size = 0;
    QString status; ///< plain English ("Downloading", "Ready", ...)
    int statusCode = 0; ///< 0..15, see AllDebrid docs
    qint64 downloaded = 0;
    qint64 downloadSpeed = 0;
    int seeders = 0;
};

/// One leaf file extracted from POST /v4/magnet/files. The
/// recursive `files: [{n, s?, l?, e?}]` tree from the API is
/// flattened by `AllDebridParse::parseMagnetFiles`; folder paths
/// are joined with `/` so `path` looks like `Sub/foo.mkv`.
struct AdMagnetFile {
    QString path;
    qint64 bytes = 0;
    QUrl hosterLink; ///< https://alldebrid.com/f/...
};

/// Result of POST /v4/link/unlock when the link is immediately
/// available. When AllDebrid returns a `delayed` id instead, the
/// client polls /v4/link/delayed and surfaces the final URL through
/// this same struct.
struct AdUnlockedLink {
    QUrl download; ///< direct hoster URL Kinema streams from
    QString filename;
    qint64 fileSize = 0;
    QString host;
    /// Internal: when the upstream response only carries a
    /// `delayed` id and no `link` yet, the client populates this so
    /// it can poll. Empty for fully-resolved responses.
    qint64 delayedId = 0;
};

} // namespace kinema::domain

Q_DECLARE_METATYPE(kinema::domain::AllDebridUser)
Q_DECLARE_METATYPE(kinema::domain::AdAddMagnetResult)
Q_DECLARE_METATYPE(kinema::domain::AdMagnetStatus)
Q_DECLARE_METATYPE(kinema::domain::AdMagnetFile)
Q_DECLARE_METATYPE(kinema::domain::AdUnlockedLink)
