// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/sources/DebridHttpMediaSource.h"

namespace kinema::api {
class AllDebridClient;
}

namespace kinema::playback::sources {

class DebridResolver;

/**
 * `MediaSourcePort` over `AllDebridResolver` +
 * `HttpRangeAssetSession`. Mirrors `RealDebridMediaSource`; the only
 * difference is the credential gate on
 * `api::AllDebridClient::apiKey()`. The `open()` / `changeMode()`
 * lifecycle is inherited from `DebridHttpMediaSource`.
 */
class AllDebridMediaSource : public DebridHttpMediaSource
{
public:
    AllDebridMediaSource(core::HttpClient& http,
        api::AllDebridClient& ad,
        DebridResolver& resolver,
        core::MediaCache& cache,
        const config::TorrentStreamingSettings& settings);
    ~AllDebridMediaSource() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::AllDebridHttp;
    }

    bool canHandle(const domain::Stream& s) const override;

private:
    api::AllDebridClient& m_ad;
};

} // namespace kinema::playback::sources
