// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QDateTime>
#include <QMetaType>
#include <QString>
#include <QStringList>
#include <QUrl>

namespace kinema::api {

/**
 * One subtitle candidate from OpenSubtitles' `/subtitles` search.
 *
 * `fileId` is the only id we ever send back to the API (`/download`).
 * Everything else is display metadata for the picker.
 */
struct SubtitleHit {
    QString fileId;          ///< OpenSubtitles file_id, stringified
    QString language;        ///< ISO 639-2 ("eng")
    QString languageName;    ///< "English" — populated from core::language
    QString releaseName;
    QString uploader;
    QString fileName;        ///< original upstream file_name
    int downloadCount = 0;
    double rating = 0.0;
    bool hearingImpaired = false;
    bool foreignPartsOnly = false;
    bool moviehashMatch = false; ///< true when our hash matched the upload's
    QString format;          ///< "srt" / "vtt" / "ass"
    int fps = 0;
};

/**
 * Filter set sent to `OpenSubtitlesClient::search`. `key.imdbId` is
 * required; `season` / `episode` come along for series. `moviehash`
 * is best-effort: when set, the API will flag matching uploads with
 * `moviehash_match: true` but the result set is unaffected.
 */
struct SubtitleSearchQuery {
    PlaybackKey key;
    QStringList languages;
    QString hearingImpaired = QStringLiteral("off");  // off|include|only
    QString foreignPartsOnly = QStringLiteral("off"); // off|include|only
    QString releaseFilter; ///< client-side substring filter
    QString moviehash;     ///< empty = don't pass to the API
};

/**
 * Result of `POST /download` — a short-lived signed URL plus quota
 * metadata. `link` is single-use; download immediately.
 */
struct SubtitleDownloadTicket {
    QUrl link;
    QString fileName;
    QString format;
    int remaining = 0;
    QDateTime resetAt; // UTC
};

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::SubtitleHit)
Q_DECLARE_METATYPE(kinema::api::SubtitleSearchQuery)
Q_DECLARE_METATYPE(kinema::api::SubtitleDownloadTicket)
