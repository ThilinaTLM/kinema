// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "download/DownloadBackend.h"

namespace kinema::core {
class MediaCache;
}

namespace kinema::torrent {
class TorrentStreamingService;
}

namespace kinema::download {

/**
 * `DownloadBackend` adapter over `torrent::TorrentStreamingService`.
 * Translates the manager's mode-based contract into engine calls:
 *
 *   - `OnDemand` -> `prepareSession(PrepareMode::Streaming)` plus
 *     idle-stop. `keepAlive` stays off so the session stops when
 *     no consumer is attached.
 *   - `Full`     -> `prepareSession(PrepareMode::Background)` plus
 *     `setKeepAlive(true)` so the engine never auto-stops it.
 *
 * `changeMode(OnDemand -> Full)` reuses the existing handle via
 * `TorrentStreamingService::promoteToFull(infoHash)` instead of
 * tearing the session down.
 */
class TorrentBackend final : public DownloadBackend
{
public:
    TorrentBackend(torrent::TorrentStreamingService& engine,
        core::MediaCache& cache);
    ~TorrentBackend() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::Torrent;
    }

    bool canHandle(const domain::Stream& s) const override;

    QCoro::Task<std::unique_ptr<AssetSession>> open(
        const domain::AssetRef& ref,
        const domain::Stream& s,
        const domain::PlaybackContext& ctx,
        domain::DownloadMode mode) override;

    void changeMode(AssetSession& session,
        domain::DownloadMode newMode) override;

private:
    torrent::TorrentStreamingService& m_engine;
    core::MediaCache& m_cache;
};

} // namespace kinema::download
