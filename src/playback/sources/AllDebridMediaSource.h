// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/MediaSourcePort.h"

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
}

namespace kinema::playback::sources {

/**
 * `MediaSourcePort` over `AllDebridResolver` +
 * `HttpRangeAssetSession`. Long-term replacement for
 * `download::AllDebridBackend`. Mirrors `RealDebridMediaSource`
 * exactly; the only difference is the gate on
 * `api::AllDebridClient::apiKey()`.
 */
class AllDebridMediaSource : public ports::MediaSourcePort
{
public:
    AllDebridMediaSource(core::HttpClient& http,
        api::AllDebridClient& ad,
        kinema::download::DebridResolver& resolver,
        core::MediaCache& cache,
        const config::DownloadSettings& settings);
    ~AllDebridMediaSource() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::AllDebridHttp;
    }

    bool canHandle(const domain::Stream& s) const override;

    QCoro::Task<ports::OpenedSession> open(const domain::AssetRef& ref,
        const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadMode mode) override;

    void changeMode(ports::ByteRangeSource& session,
        domain::DownloadMode newMode) override;

private:
    core::HttpClient& m_http;
    api::AllDebridClient& m_ad;
    kinema::download::DebridResolver& m_resolver;
    core::MediaCache& m_cache;
    const config::DownloadSettings& m_settings;
};

} // namespace kinema::playback::sources
