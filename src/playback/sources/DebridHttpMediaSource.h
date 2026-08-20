// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/MediaSourcePort.h"

namespace kinema::core {
class HttpClient;
class MediaCache;
}

namespace kinema::config {
class TorrentStreamingSettings;
}

namespace kinema::playback::sources {

class DebridResolver;

/**
 * Shared `MediaSourcePort` base for the debrid providers that back
 * their sessions with `HttpRangeAssetSession` (Real-Debrid and
 * AllDebrid). Implements the identical `open()` / `changeMode()`
 * lifecycle; each provider supplies only its credential gate
 * (`canHandle`) and backend `kind()`.
 */
class DebridHttpMediaSource : public ports::MediaSourcePort
{
public:
    ~DebridHttpMediaSource() override;

    QCoro::Task<ports::OpenedSession> open(const domain::AssetRef& ref,
        const domain::Stream& stream, const domain::PlaybackContext& ctx,
        domain::DownloadMode mode) override;

    void changeMode(ports::ByteRangeSource& session,
        domain::DownloadMode newMode) override;

protected:
    DebridHttpMediaSource(core::HttpClient& http,
        DebridResolver& resolver,
        core::MediaCache& cache,
        const config::TorrentStreamingSettings& settings,
        const char* logName);

    core::HttpClient& m_http;
    DebridResolver& m_resolver;
    core::MediaCache& m_cache;
    const config::TorrentStreamingSettings& m_settings;
    const char* m_logName;
};

} // namespace kinema::playback::sources
