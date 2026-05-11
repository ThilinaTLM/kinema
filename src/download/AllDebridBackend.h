// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "download/DownloadBackend.h"

namespace kinema::api {
class AllDebridClient;
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
 * `DownloadBackend` adapter over `AllDebridResolver` +
 * `HttpAssetSession`. Mirrors `RealDebridBackend` exactly; the only
 * differences are:
 *
 *   - `kind()` reports `AllDebridHttp`.
 *   - `canHandle()` checks the AllDebrid client's `apiKey()`
 *     instead of the RD client's `token()`.
 *
 * The HTTP chunked-fetch loop, mode plumbing and prefetch promotion
 * all live in `HttpAssetSession` and are shared verbatim.
 */
class AllDebridBackend final : public DownloadBackend
{
public:
    AllDebridBackend(core::HttpClient& http,
        api::AllDebridClient& ad,
        DebridResolver& resolver,
        core::MediaCache& cache,
        const config::DownloadSettings& settings);
    ~AllDebridBackend() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::AllDebridHttp;
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
    api::AllDebridClient& m_ad;
    DebridResolver& m_resolver;
    core::MediaCache& m_cache;
    const config::DownloadSettings& m_settings;
};

} // namespace kinema::download
