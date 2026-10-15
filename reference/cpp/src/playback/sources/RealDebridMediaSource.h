// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/sources/DebridHttpMediaSource.h"

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
    DebridResolver& m_resolver;
};

} // namespace kinema::playback::sources
