// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "download/DownloadBackend.h"

namespace kinema::api {
class RealDebridClient;
}

namespace kinema::core {
class HttpClient;
class MediaCache;
}

namespace kinema::config {
class DownloadSettings;
}

namespace kinema::download {

class DebridResolver;

/**
 * `DownloadBackend` adapter over `RealDebridResolver` +
 * `HttpAssetSession`. Translates the manager's mode contract:
 *
 *   - `OnDemand` -> create the session (which kicks off
 *     `ensureResolved()`), but do not start `prefetchAll()`. The
 *     player's range fetches drive cache fill.
 *   - `Full`     -> create the session, then fire-and-forget
 *     `prefetchAll()` so the chunk loop fills the file in the
 *     background regardless of playback.
 *
 * `changeMode(OnDemand -> Full)` flips the session's mode flag and
 * starts `prefetchAll()`. The reverse just clears the mode flag;
 * the prefetch loop honours that at the next chunk boundary.
 */
class RealDebridBackend final : public DownloadBackend
{
public:
    RealDebridBackend(core::HttpClient& http,
        api::RealDebridClient& rd,
        DebridResolver& resolver,
        core::MediaCache& cache,
        const config::DownloadSettings& settings);
    ~RealDebridBackend() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::RealDebridHttp;
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
    core::HttpClient& m_http;
    api::RealDebridClient& m_rd;
    DebridResolver& m_resolver;
    core::MediaCache& m_cache;
    const config::DownloadSettings& m_settings;
};

} // namespace kinema::download
