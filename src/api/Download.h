// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QDateTime>
#include <QMetaType>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::api {

/// Which downloader backend a given asset is served from.
enum class DownloadBackendKind {
    Torrent, ///< libtorrent-backed local progressive download.
    RealDebridHttp, ///< RD hoster URL fetched in chunks into a sparse file.
};

/// Lifecycle state for a `DownloadItem`.
enum class DownloadState {
    Queued,      ///< persisted but no work has started yet
    Preparing,   ///< resolving (e.g. RD addMagnet/instantAvailability)
    Downloading, ///< bytes are flowing
    Streaming,   ///< serving the asset to the player while still incomplete
    Completed,   ///< all bytes available locally
    Failed,      ///< terminal error; `lastError` populated
    Paused,      ///< user-initiated pause
    Cancelled,   ///< user-initiated cancellation; files may still exist
};

/// Eviction policy for an asset.
enum class CacheDisposition {
    Ephemeral, ///< opportunistic playback cache; evictable
    Pinned,    ///< explicit user download; never auto-evicted
};

/**
 * Stable identity of a local downloadable asset. Does NOT contain the
 * volatile RD direct URL, by design \u2014 we re-resolve from this struct
 * each time we need a fresh hoster URL.
 *
 * Constructed from `api::Stream` + `api::PlaybackContext` by the
 * download manager.
 */
struct AssetRef {
    PlaybackKey key;
    QString infoHash; ///< lower-case hex
    QString releaseName;
    int fileIndex = -1; ///< -1 = let backend pick
    QString fileNameHint;
    QString qualityLabel;
    QString resolution;
    QString provider;
    std::optional<qint64> sizeBytes;

    /// Returns true once we have at minimum a key + a way to reach
    /// the bytes (info hash for the torrent backend).
    bool isValid() const noexcept
    {
        return key.isValid() && !infoHash.isEmpty();
    }
};

/**
 * One row in the persistent downloads table. Mirrored 1:1 to the
 * `download_items` SQLite table created by `core::DownloadStore`.
 */
struct DownloadItem {
    /// Stable id derived from the asset's identity. Used as the
    /// primary key in storage and as the cache directory name.
    QString assetId;
    DownloadBackendKind backendKind = DownloadBackendKind::Torrent;
    DownloadState state = DownloadState::Queued;
    CacheDisposition disposition = CacheDisposition::Ephemeral;

    PlaybackKey key;
    QString title;
    QString seriesTitle;
    QString episodeTitle;
    QUrl poster;

    QString infoHash;
    QString releaseName;
    int fileIndex = -1;
    QString fileNameHint;
    QString qualityLabel;
    QString resolution;
    QString provider;

    std::optional<qint64> expectedSizeBytes;
    qint64 cachedSizeBytes = 0;
    QString lastError;
    bool complete = false;

    QString localDir; ///< absolute path to the asset's cache dir

    QDateTime addedAt;
    QDateTime updatedAt;
    QDateTime lastUsedAt;

    bool isPinned() const noexcept
    {
        return disposition == CacheDisposition::Pinned;
    }

    /// Progress in [0,1]. -1 when the expected size is unknown.
    double progressFraction() const noexcept
    {
        if (!expectedSizeBytes || *expectedSizeBytes <= 0) {
            return complete ? 1.0 : -1.0;
        }
        return qBound(0.0,
            static_cast<double>(cachedSizeBytes)
                / static_cast<double>(*expectedSizeBytes),
            1.0);
    }
};

/// Build a stable asset id from `infoHash` + `fileIndex`. Falls back
/// to a hash of the release name when the info hash is empty (which
/// can happen for direct-URL-only candidates we still want to cache).
QString assetIdFor(const AssetRef& ref);

/// Lift an `AssetRef` from a discovery `Stream` and a play context.
AssetRef assetRefFor(const Stream& s, const PlaybackContext& ctx);

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::AssetRef)
Q_DECLARE_METATYPE(kinema::api::DownloadItem)
Q_DECLARE_METATYPE(kinema::api::DownloadBackendKind)
Q_DECLARE_METATYPE(kinema::api::DownloadState)
Q_DECLARE_METATYPE(kinema::api::CacheDisposition)
