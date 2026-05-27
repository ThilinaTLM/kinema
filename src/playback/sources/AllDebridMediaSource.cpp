// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/AllDebridMediaSource.h"

#include "api/AllDebridClient.h"
#include "core/persistence/MediaCache.h"
#include "download/DebridResolver.h"
#include "kinema_log_download.h"
#include "playback/sources/HttpRangeAssetSession.h"

#include <memory>
#include <utility>

namespace kinema::playback::sources {

AllDebridMediaSource::AllDebridMediaSource(core::HttpClient& http,
    api::AllDebridClient& ad,
    kinema::download::DebridResolver& resolver,
    core::MediaCache& cache,
    const config::DownloadSettings& settings)
    : m_http(http)
    , m_ad(ad)
    , m_resolver(resolver)
    , m_cache(cache)
    , m_settings(settings)
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

QCoro::Task<ports::OpenedSession> AllDebridMediaSource::open(
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

    co_await raw->ensureResolved();

    if (mode == domain::DownloadMode::Full) {
        auto task = raw->prefetchAll();
        Q_UNUSED(task);
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "AllDebridMediaSource::open assetId=\"" << assetId
        << "\" mode=" << static_cast<int>(mode)
        << " fileSize=" << raw->fileSize();

    ports::OpenedSession out;
    out.assetId = assetId;
    out.session = std::move(session);
    co_return out;
}

void AllDebridMediaSource::changeMode(ports::ByteRangeSource& session,
    domain::DownloadMode newMode)
{
    auto* http = dynamic_cast<HttpRangeAssetSession*>(&session);
    if (!http) {
        qCWarning(KINEMA_DOWNLOAD)
            << "AllDebridMediaSource::changeMode on non-HTTP session";
        return;
    }
    if (http->mode() == newMode) {
        return;
    }
    http->setMode(newMode);
    if (newMode == domain::DownloadMode::Full) {
        auto task = http->prefetchAll();
        Q_UNUSED(task);
    }
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "AllDebridMediaSource::changeMode assetId=\""
        << http->assetId()
        << "\" -> mode=" << static_cast<int>(newMode);
}

} // namespace kinema::playback::sources
