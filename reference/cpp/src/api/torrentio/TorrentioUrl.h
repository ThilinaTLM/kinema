// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::core::torrentio {

/**
 * Structured fields recovered from a Torrentio `/resolve/...` URL. When
 * Torrentio is configured with a debrid provider (`alldebrid=<KEY>`,
 * `realdebrid=<TOKEN>`, ...), individual stream rows come back with only
 * a `url` field pointing at the addon's resolve endpoint — the
 * structured `infoHash` / `fileIdx` fields are dropped. We recover them
 * from the URL path so the unified downloader and local debrid
 * resolvers can still serve the actual playback URL.
 */
struct ResolveUrlInfo {
    /// 40-char lowercase hex info hash.
    QString infoHash;
    /// Non-negative file index inside the torrent.
    int fileIndex = -1;
    /// Percent-decoded filename hint (`"The Rookie S02E01.mkv"`).
    QString fileNameHint;
};

/**
 * Parse a Torrentio resolve URL into its structured pieces.
 *
 * The recognised shape is:
 *
 *   <scheme>://<host>/resolve/<provider>/<token>/<hash40>/<file>/<idx>/<file>
 *
 * `host` is accepted when it equals `torrentio.strem.fun`, is a subdomain
 * of it, or contains the substring `"torrentio"` (so self-hosted mirrors
 * like `torrentio.example.com` keep working). The `provider` and `token`
 * segments are read but never returned — we deliberately drop the
 * credential at the boundary so it cannot leak into logs or the clipboard.
 *
 * Returns `std::nullopt` on any shape mismatch: wrong scheme, wrong host,
 * non-`resolve` first segment, wrong number of segments, non-hex or
 * wrong-length hash, or a non-integer file index. Callers should treat
 * the URL as opaque on a `nullopt` result and leave the stream's
 * `directUrl` alone.
 */
std::optional<ResolveUrlInfo> parseResolveUrl(const QUrl& u);

} // namespace kinema::core::torrentio
