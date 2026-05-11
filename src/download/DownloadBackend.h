// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QCoro/QCoroTask>

#include <memory>

namespace kinema::download {

class AssetSession;

/**
 * Strategy interface for the unified download manager. Concrete
 * implementations (torrent, Real-Debrid HTTP, future hosts) own
 * their backend-specific state and produce `AssetSession` instances
 * that the manager registers with the localhost media server.
 *
 * `DownloadManager` itself stays backend-agnostic: it asks
 * `BackendSelector` for the right backend, then drives it via this
 * interface. Adding a new transport (plain HTTP, IPFS, AllDebrid)
 * is a matter of writing one of these and registering it.
 */
class DownloadBackend
{
public:
    virtual ~DownloadBackend() = default;

    /// Stable enum tag used by selection rules and UI.
    virtual api::DownloadBackendKind kind() const noexcept = 0;

    /// True if this backend is configured + the given Stream has
    /// the affordances this backend needs (info hash for torrent;
    /// RD token + cached/direct URL for Real-Debrid).
    virtual bool canHandle(const api::Stream& s) const = 0;

    /// Realise a session for the given asset. The backend honours
    /// `mode`: OnDemand sets up streaming-buffer behaviour and
    /// idle-stop; Full sets up eager whole-file download with no
    /// idle-stop. Throws on unrecoverable failure; the manager
    /// converts thrown exceptions into a `Failed` row update.
    virtual QCoro::Task<std::unique_ptr<AssetSession>> open(
        const api::AssetRef& ref,
        const api::Stream& s,
        const api::PlaybackContext& ctx,
        api::DownloadMode mode) = 0;

    /// Switch an existing session's policy without recreating it.
    /// Implementations must be idempotent: calling with the
    /// session's current mode is a no-op. Used by upgrade in place
    /// (`OnDemand` -> `Full`) and pause/resume edge cases.
    virtual void changeMode(AssetSession& session,
        api::DownloadMode newMode) = 0;
};

} // namespace kinema::download
