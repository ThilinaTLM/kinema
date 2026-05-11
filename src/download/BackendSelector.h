// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Debrid.h"
#include "api/Download.h"
#include "api/Media.h"

#include <memory>
#include <optional>
#include <vector>

namespace kinema::download {

class DownloadBackend;

/**
 * Picks a `DownloadBackend` for a given `Stream`. Backends are
 * registered in priority order; selection prefers the optional
 * override, then walks the registered list and returns the first
 * one whose `canHandle()` says yes.
 *
 * The selector owns the registered backends so `DownloadManager`
 * can hold a single `unique_ptr<BackendSelector>` instead of one
 * per backend kind.
 */
class BackendSelector
{
public:
    BackendSelector();
    ~BackendSelector();

    BackendSelector(const BackendSelector&) = delete;
    BackendSelector& operator=(const BackendSelector&) = delete;

    /// Add a backend to the selection list. Insertion order is
    /// the priority order; debrid backends should be registered
    /// before the torrent backend. The selector still gates debrid
    /// backends on `setActiveDebridProvider`.
    void registerBackend(std::unique_ptr<DownloadBackend> backend);

    /// Track which debrid provider the user chose as active. The
    /// non-active debrid backend is skipped during default routing
    /// (override paths still work). `DebridProvider::None` skips
    /// every debrid backend, leaving only the torrent backend.
    void setActiveDebridProvider(api::DebridProvider p) noexcept
    {
        m_activeDebrid = p;
    }
    api::DebridProvider activeDebridProvider() const noexcept
    {
        return m_activeDebrid;
    }

    /// Find a backend for the given stream. When `override` is
    /// provided the matching backend must report `canHandle`;
    /// otherwise the call throws so the caller surfaces the failure
    /// rather than silently routing through a different backend.
    /// When no override is given, iterate by priority.
    /// Returns `nullptr` when no registered backend can serve the
    /// stream (no override given, no backend `canHandle` it).
    DownloadBackend* select(const api::Stream& s,
        std::optional<api::DownloadBackendKind> override = std::nullopt) const;

    /// Look up a previously registered backend by kind. Used by
    /// resume paths that already know which backend produced a
    /// row.
    DownloadBackend* find(api::DownloadBackendKind kind) const;

private:
    std::vector<std::unique_ptr<DownloadBackend>> m_backends;
    api::DebridProvider m_activeDebrid = api::DebridProvider::None;
};

} // namespace kinema::download
