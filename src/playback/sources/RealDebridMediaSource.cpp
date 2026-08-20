// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/RealDebridMediaSource.h"

#include "api/RealDebridClient.h"
#include "playback/sources/DebridResolver.h"

namespace kinema::playback::sources {

RealDebridMediaSource::RealDebridMediaSource(core::HttpClient& http,
    api::RealDebridClient& rd,
    DebridResolver& resolver,
    core::MediaCache& cache,
    const config::TorrentStreamingSettings& settings)
    : DebridHttpMediaSource(http, resolver, cache, settings,
          "RealDebridMediaSource")
    , m_rd(rd)
{
}

RealDebridMediaSource::~RealDebridMediaSource() = default;

bool RealDebridMediaSource::canHandle(const domain::Stream& s) const
{
    if (m_rd.token().isEmpty()) {
        return false;
    }
    // When RD is configured, route every stream with an actionable
    // identifier through it. RealDebridResolver works for any
    // infohash via addMagnet -> selectFiles -> unrestrictLink, and a
    // pre-resolved direct URL is unrestricted directly. RD's
    // instantAvailability endpoint is no longer reliable, so we no
    // longer probe "is this cached?" before routing.
    return !s.infoHash.isEmpty() || !s.directUrl.isEmpty();
}

} // namespace kinema::playback::sources
