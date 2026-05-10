// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/AllDebridBackend.h"

#include "api/AllDebridClient.h"
#include "core/MediaCache.h"
#include "download/DebridResolver.h"
#include "download/HttpAssetSession.h"
#include "kinema_log_download.h"

namespace kinema::download {

AllDebridBackend::AllDebridBackend(core::HttpClient& http,
    api::AllDebridClient& ad,
    DebridResolver& resolver,
    core::MediaCache& cache,
    const config::DownloadSettings& settings)
    : m_http(http)
    , m_ad(ad)
    , m_resolver(resolver)
    , m_cache(cache)
    , m_settings(settings)
{
}

AllDebridBackend::~AllDebridBackend() = default;

bool AllDebridBackend::canHandle(const api::Stream& s) const
{
    if (m_ad.apiKey().isEmpty()) {
        return false;
    }
    // Same routing rule as the RD backend: when AllDebrid is
    // configured, every stream with an actionable identifier flows
    // through it. The resolver works for any infohash.
    return !s.infoHash.isEmpty() || !s.directUrl.isEmpty();
}

QCoro::Task<std::unique_ptr<AssetSession>> AllDebridBackend::open(
    const api::AssetRef& ref,
    const api::Stream& s,
    const api::PlaybackContext& ctx,
    api::DownloadMode mode)
{
    Q_UNUSED(s);
    Q_UNUSED(ctx);
    const auto assetId = api::assetIdFor(ref);
    m_cache.markActive(assetId);

    auto session = std::make_unique<HttpAssetSession>(m_http, m_resolver,
        m_settings, ref, assetId,
        m_cache.assetDir(assetId).absolutePath());
    session->setMode(mode);
    auto* raw = session.get();

    co_await raw->ensureResolved();

    if (mode == api::DownloadMode::Full) {
        auto task = raw->prefetchAll();
        Q_UNUSED(task);
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "AllDebridBackend::open assetId=\"" << assetId
        << "\" mode=" << static_cast<int>(mode)
        << " fileSize=" << raw->fileSize();

    co_return session;
}

void AllDebridBackend::changeMode(AssetSession& session,
    api::DownloadMode newMode)
{
    if (session.mode() == newMode) {
        return;
    }
    auto* http = qobject_cast<HttpAssetSession*>(&session);
    if (!http) {
        qCWarning(KINEMA_DOWNLOAD)
            << "AllDebridBackend::changeMode called on non-HTTP session";
        return;
    }
    http->setMode(newMode);
    if (newMode == api::DownloadMode::Full) {
        auto task = http->prefetchAll();
        Q_UNUSED(task);
    }
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "AllDebridBackend::changeMode assetId=\"" << session.assetId()
        << "\" -> mode=" << static_cast<int>(newMode);
}

} // namespace kinema::download
