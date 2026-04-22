// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QDateTime>
#include <QMetaType>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::api {

/**
 * Canonical media identity used across history, resume, and the
 * in-flight playback pipeline. Independent of the specific release /
 * stream URL so progress survives stream swaps.
 *
 * Encoded as "ttXXXXXXX" for movies and "ttXXXXXXX:S:E" for series
 * episodes \u2014 mirroring Torrentio's streamId shape.
 */
struct PlaybackKey {
    MediaKind kind = MediaKind::Movie;
    QString imdbId;
    std::optional<int> season;
    std::optional<int> episode;

    /// On-disk key: "ttXXXXXXX" for movies, "ttXXXXXXX:S:E" for
    /// series episodes. Returns an empty string if imdbId is empty.
    QString storageKey() const;

    bool isValid() const { return !imdbId.isEmpty(); }

    bool operator==(const PlaybackKey& other) const
    {
        return kind == other.kind
            && imdbId == other.imdbId
            && season == other.season
            && episode == other.episode;
    }
    bool operator!=(const PlaybackKey& other) const { return !(*this == other); }
};

/**
 * Persisted reference to a specific Torrentio release. Stable across
 * Real-Debrid sessions because `directUrl` is NEVER stored \u2014 RD
 * unrestrict URLs expire. We recover a currently-valid URL on resume
 * by re-fetching Torrentio and matching `infoHash`.
 */
struct HistoryStreamRef {
    QString infoHash;
    QString releaseName;
    QString resolution;
    QString qualityLabel;
    std::optional<qint64> sizeBytes;
    QString provider;

    /// Build a history reference from a live Torrentio stream.
    static HistoryStreamRef fromStream(const Stream& s);

    /// True when `infoHash` matches. Empty-hash refs never match.
    bool matches(const Stream& s) const;

    bool isEmpty() const { return infoHash.isEmpty(); }
};

/**
 * One history row. A plain struct, ferried by value through the
 * store/controller/UI boundaries.
 */
struct HistoryEntry {
    PlaybackKey key;
    QString title;          ///< display title; series title for a series entry
    QString seriesTitle;    ///< empty for movies
    QString episodeTitle;   ///< empty for movies
    QUrl poster;
    double positionSec = 0.0;
    double durationSec = 0.0;
    bool finished = false;
    QDateTime lastWatchedAt;    ///< UTC
    HistoryStreamRef lastStream;

    /// Progress fraction in [0, 1], or -1 when duration is unknown.
    double progressFraction() const
    {
        if (durationSec <= 0.0) {
            return -1.0;
        }
        return qBound(0.0, positionSec / durationSec, 1.0);
    }
};

/**
 * Passed through every play path so the history layer can both
 * record an entry and seed resume-from into the embedded player.
 *
 * Field ownership:
 * - Caller (DetailPane / SeriesDetailPane / resume flow) fills `key`,
 *   display fields, and `poster`.
 * - `StreamActions::play` fills `streamRef` from the chosen Stream
 *   and asks `HistoryController::resumeSecondsFor()` to fill
 *   `resumeSeconds`.
 */
struct PlaybackContext {
    PlaybackKey key;
    QString title;
    QString seriesTitle;
    QString episodeTitle;
    QUrl poster;
    HistoryStreamRef streamRef;
    std::optional<qint64> resumeSeconds;
};

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::PlaybackKey)
Q_DECLARE_METATYPE(kinema::api::HistoryStreamRef)
Q_DECLARE_METATYPE(kinema::api::HistoryEntry)
Q_DECLARE_METATYPE(kinema::api::PlaybackContext)
