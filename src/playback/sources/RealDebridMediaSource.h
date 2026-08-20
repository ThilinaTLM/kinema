// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/sources/DebridHttpMediaSource.h"

namespace kinema::api {
class RealDebridClient;
}

namespace kinema::playback::sources {

class DebridResolver;

/**
 * `MediaSourcePort` over `RealDebridResolver` +
 * `HttpRangeAssetSession`.
 *
 * The `open()` / `changeMode()` lifecycle is inherited from
 * `DebridHttpMediaSource`; this class only adds the Real-Debrid
 * credential gate (`canHandle`) and backend `kind()`.
 */
class RealDebridMediaSource : public DebridHttpMediaSource
{
public:
    RealDebridMediaSource(core::HttpClient& http,
        api::RealDebridClient& rd,
        DebridResolver& resolver,
        core::MediaCache& cache,
        const config::TorrentStreamingSettings& settings);
    ~RealDebridMediaSource() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::RealDebridHttp;
    }

    bool canHandle(const domain::Stream& s) const override;

private:
    api::RealDebridClient& m_rd;
};

} // namespace kinema::playback::sources
