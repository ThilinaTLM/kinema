// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/RealDebridBackend.h"

#include "api/RealDebridClient.h"
#include "core/persistence/MediaCache.h"
#include "download/DebridResolver.h"
#include "download/HttpAssetSession.h"
#include "kinema_log_download.h"

namespace kinema::download {

RealDebridBackend::RealDebridBackend(core::HttpClient& http,
    api::RealDebridClient& rd,
    DebridResolver& resolver,
    core::MediaCache& cache,
    const config::DownloadSettings& settings)
    : m_http(http)
    , m_rd(rd)
    , m_resolver(resolver)
    , m_cache(cache)
    , m_settings(settings)
{
}

RealDebridBackend::~RealDebridBackend() = default;

bool RealDebridBackend::canHandle(const domain::Stream& s) const
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

QCoro::Task<std::unique_ptr<AssetSession>> RealDebridBackend::open(
    const domain::AssetRef& ref,
    const domain::Stream& s,
    const domain::PlaybackContext& ctx,
    domain::DownloadMode mode)
{
    Q_UNUSED(s);
    Q_UNUSED(ctx);
    const auto assetId = domain::assetIdFor(ref);
    m_cache.markActive(assetId);

    auto session = std::make_unique<HttpAssetSession>(m_http, m_resolver,
        m_settings, ref, assetId,
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
        // `failed` signal which the manager has already wired.
        auto task = raw->prefetchAll();
        Q_UNUSED(task);
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "RealDebridBackend::open assetId=\"" << assetId
        << "\" mode=" << static_cast<int>(mode)
        << " fileSize=" << raw->fileSize();

    co_return session;
}

void RealDebridBackend::changeMode(AssetSession& session,
    domain::DownloadMode newMode)
{
    if (session.mode() == newMode) {
        return;
    }
    auto* http = qobject_cast<HttpAssetSession*>(&session);
    if (!http) {
        qCWarning(KINEMA_DOWNLOAD)
            << "RealDebridBackend::changeMode called on non-HTTP session";
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
        << "RealDebridBackend::changeMode assetId=\"" << session.assetId()
        << "\" -> mode=" << static_cast<int>(newMode);
}

} // namespace kinema::download
