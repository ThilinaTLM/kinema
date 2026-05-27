// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/MediaSourcePort.h"

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

namespace kinema::playback::sources {

class DebridResolver;

/**
 * `MediaSourcePort` over `RealDebridResolver` +
 * `HttpRangeAssetSession`.
 *
 * Mode contract mirrors the legacy backend exactly:
 *
 *   - `OnDemand` -> create the session (which kicks off
 *     `ensureResolved()`), but do not start `prefetchAll()`. The
 *     player's range fetches drive cache fill.
 *   - `Full`     -> create the session, then fire-and-forget
 *     `prefetchAll()` so the chunk loop fills the file in the
 *     background regardless of playback.
 *
 * `changeMode(OnDemand -> Full)` flips the session's mode flag
 * and starts `prefetchAll()`; the reverse just clears the mode
 * flag (the prefetch loop honours it at the next chunk boundary).
 */
class RealDebridMediaSource : public ports::MediaSourcePort
{
public:
    RealDebridMediaSource(core::HttpClient& http,
        api::RealDebridClient& rd,
        DebridResolver& resolver,
        core::MediaCache& cache,
        const config::DownloadSettings& settings);
    ~RealDebridMediaSource() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::RealDebridHttp;
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
    api::RealDebridClient& m_rd;
    DebridResolver& m_resolver;
    core::MediaCache& m_cache;
    const config::DownloadSettings& m_settings;
};

} // namespace kinema::playback::sources
