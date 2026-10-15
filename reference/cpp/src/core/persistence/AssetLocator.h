// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"

#include <QString>

namespace kinema::core {

/**
 * Where a download row's bytes actually live on disk.
 *
 * The two backends do not share a payload tree:
 *
 *   - Torrent rows are written by libtorrent, whose `save_path` is
 *     `cache::torrentsDir()/<infoHash>` (see `LibtorrentClient::add`).
 *     The torrent's own directory layout is nested underneath, so a
 *     season pack lands several assets in one directory.
 *   - Debrid HTTP rows are written by `HttpRangeAssetSession` into
 *     `cache::mediaDir()/<assetId>`.
 *
 * `MediaCache::assetDir()` is bookkeeping-only for torrent rows — it
 * holds the `.last-used` / `.pinned` markers and never the media. The
 * `DownloadItem::localDir` column points there for *both* backends,
 * which is why it cannot be shown to the user directly.
 */
struct AssetLocation {
    /// Directory to open in the file manager. Empty when the row
    /// carries nothing we can resolve.
    QString dir;
    /// Exact payload file to select inside `dir`, when we can pin it
    /// down. Empty for a pack whose member file stays ambiguous.
    QString file;
    /// True when `dir` exists on disk right now.
    bool exists = false;

    /// The most specific path we resolved — the file when known,
    /// otherwise the directory.
    QString bestPath() const { return file.isEmpty() ? dir : file; }
};

/**
 * Resolve the real on-disk location for `item`, ignoring the stored
 * `localDir` column in favour of the backend's actual payload tree.
 *
 * Directory resolution is pure path arithmetic. The exact-file search
 * walks `dir`, so call it on user action (reveal / copy path), not
 * from a model role that repaints on every progress tick.
 */
AssetLocation locateAsset(const domain::DownloadItem& item);

/**
 * Directory half of `locateAsset` with no filesystem walk. Cheap
 * enough to call per row per repaint.
 */
QString locateAssetDir(const domain::DownloadItem& item);

} // namespace kinema::core
