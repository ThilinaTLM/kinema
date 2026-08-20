// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/AllDebridMediaSource.h"

#include "api/AllDebridClient.h"
#include "playback/sources/DebridResolver.h"

namespace kinema::playback::sources {

AllDebridMediaSource::AllDebridMediaSource(core::HttpClient& http,
    api::AllDebridClient& ad,
    DebridResolver& resolver,
    core::MediaCache& cache,
    const config::TorrentStreamingSettings& settings)
    : DebridHttpMediaSource(http, resolver, cache, settings,
          "AllDebridMediaSource")
    , m_ad(ad)
{
}

AllDebridMediaSource::~AllDebridMediaSource() = default;

bool AllDebridMediaSource::canHandle(const domain::Stream& s) const
{
    if (m_ad.apiKey().isEmpty()) {
        return false;
    }
    // Same routing rule as the RD source: when AllDebrid is
    // configured, every stream with an actionable identifier flows
    // through it. The resolver works for any infohash.
    return !s.infoHash.isEmpty() || !s.directUrl.isEmpty();
}

} // namespace kinema::playback::sources
