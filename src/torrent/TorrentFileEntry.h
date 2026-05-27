// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/MediaFile.h"

namespace kinema::torrent {

/**
 * Per-file entry inside a torrent (or, generically, inside any
 * media-source session).
 *
 * Historically this was a torrent-specific struct living in
 * `MediaFileSelector.h` with just `{ index, path, size }`. The
 * refactor unified it with the backend-agnostic
 * `domain::MediaFileEntry` so policy code (selection, adjacency,
 * series auto-next) can operate on a single shape regardless of
 * whether the entries came from libtorrent or a debrid resolver.
 *
 * The alias is kept under the original name so existing callsites
 * (`torrent::TorrentFileEntry { i, path, size }`) continue to
 * compile unchanged — the additional `playable` field defaults to
 * `true`.
 */
using TorrentFileEntry = domain::MediaFileEntry;

} // namespace kinema::torrent
