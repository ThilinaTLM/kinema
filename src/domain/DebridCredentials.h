// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Debrid.h"

#include <QString>

namespace kinema::domain {

/**
 * Snapshot of the active debrid credential at one moment in time.
 *
 * `provider == DebridProvider::None` or `token.isEmpty()` means
 * "no debrid" — callers must not append any debrid segment to the
 * outgoing URL. The resolver collapses an empty-token case to
 * `{None, {}}` so consumers only have to check `provider`.
 */
struct ActiveDebrid {
    DebridProvider provider = DebridProvider::None;
    QString token;
};

/**
 * Stable URL token (`"realdebrid"` / `"alldebrid"`) used by both
 * Torrentio and Peerflix addons in their pipe-separated config
 * segment. Returns an empty string for `DebridProvider::None`.
 *
 * Distinct from `providerToString()` (which is the persistence
 * token) only by coincidence — the upstream addons happen to use
 * the same strings. Keeping a dedicated accessor here means that
 * if a future provider uses a different URL slug we can map it
 * without changing the persistence layer.
 */
QString providerToUrlToken(DebridProvider);

/**
 * Read-only port for "what debrid credential should I send right
 * now?". Implementations must be cheap to call (no I/O) — the
 * concrete `controllers::DebridCredentialsResolver` is backed by
 * already-cached in-memory state (`TokenController`'s `const
 * QString&` aliases + `DebridSettings::activeProvider()`).
 *
 * Mirrors the port-style of `domain::Indexer`: `api/` consumers
 * (`TorrentioIndexer`, `PeerflixIndexer`) only know this
 * interface, so they stay free of `controllers/` includes.
 */
class DebridCredentialsProvider
{
public:
    virtual ~DebridCredentialsProvider() = default;
    virtual ActiveDebrid active() const = 0;
};

} // namespace kinema::domain
