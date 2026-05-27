// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/RealDebridMediaSource.h"

#include "api/RealDebridClient.h"
#include "core/persistence/MediaCache.h"
#include "download/DebridResolver.h"
#include "kinema_log_download.h"
#include "playback/sources/HttpRangeAssetSession.h"

#include <memory>
#include <utility>

namespace kinema::playback::sources {

RealDebridMediaSource::RealDebridMediaSource(core::HttpClient& http,
    api::RealDebridClient& rd,
    kinema::download::DebridResolver& resolver,
    core::MediaCache& cache,
    const config::DownloadSettings& settings)
    : m_http(http)
    , m_rd(rd)
    , m_resolver(resolver)
    , m_cache(cache)
    , m_settings(settings)
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

QCoro::Task<ports::OpenedSession> RealDebridMediaSource::open(
    const domain::AssetRef& ref, const domain::Stream& stream,
    const domain::PlaybackContext& ctx, domain::DownloadMode mode)
{
    Q_UNUSED(stream);
    Q_UNUSED(ctx);
    const auto assetId = domain::assetIdFor(ref);
    m_cache.markActive(assetId);

    auto session = std::make_unique<HttpRangeAssetSession>(m_http,
        m_resolver, m_settings, ref, assetId,
        m_cache.assetDir(assetId).absolutePath());
    session->setMode(mode);
    auto* raw = session.get();

    // Resolve once up front so the caller surfaces RD failures
    // synchronously (i.e. a `Failed` row instead of silent stalls).
    co_await raw->ensureResolved();

    if (mode == domain::DownloadMode::Full) {
        // Fire and forget; the prefetch loop honours `m_paused`
        // and `m_mode` so future demotion/pause is observed at the
        // next chunk boundary. Errors propagate via the session's
        // `failed` signal which the supervisor has already wired.
        auto task = raw->prefetchAll();
        Q_UNUSED(task);
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "RealDebridMediaSource::open assetId=\"" << assetId
        << "\" mode=" << static_cast<int>(mode)
        << " fileSize=" << raw->fileSize();

    ports::OpenedSession out;
    out.assetId = assetId;
    out.session = std::move(session);
    co_return out;
}

void RealDebridMediaSource::changeMode(ports::ByteRangeSource& session,
    domain::DownloadMode newMode)
{
    auto* http = dynamic_cast<HttpRangeAssetSession*>(&session);
    if (!http) {
        qCWarning(KINEMA_DOWNLOAD)
            << "RealDebridMediaSource::changeMode on non-HTTP session";
        return;
    }
    if (http->mode() == newMode) {
        return;
    }
    http->setMode(newMode);
    if (newMode == domain::DownloadMode::Full) {
        // Wake the prefetch loop. If a previous prefetchAll() is
        // still in flight (e.g. paused mid-loop) it'll resume on
        // its next iteration; if not, this kicks a fresh one.
        auto task = http->prefetchAll();
        Q_UNUSED(task);
    }
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "RealDebridMediaSource::changeMode assetId=\""
        << http->assetId()
        << "\" -> mode=" << static_cast<int>(newMode);
}

} // namespace kinema::playback::sources
