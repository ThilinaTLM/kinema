// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/DebridHttpMediaSource.h"

#include "core/persistence/MediaCache.h"
#include "kinema_log_download.h"
#include "playback/sources/DebridResolver.h"
#include "playback/sources/HttpRangeAssetSession.h"

#include <memory>
#include <utility>

namespace kinema::playback::sources {

DebridHttpMediaSource::DebridHttpMediaSource(core::HttpClient& http,
    DebridResolver& resolver,
    core::MediaCache& cache,
    const config::TorrentStreamingSettings& settings,
    const char* logName)
    : m_http(http)
    , m_resolver(resolver)
    , m_cache(cache)
    , m_settings(settings)
    , m_logName(logName)
{
}

DebridHttpMediaSource::~DebridHttpMediaSource() = default;

QCoro::Task<ports::OpenedSession> DebridHttpMediaSource::open(
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

    // Resolve once up front so the caller surfaces provider failures
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
        << m_logName << "::open assetId=\"" << assetId
        << "\" mode=" << static_cast<int>(mode)
        << " fileSize=" << raw->fileSize();

    ports::OpenedSession out;
    out.assetId = assetId;
    out.session = std::move(session);
    co_return out;
}

void DebridHttpMediaSource::changeMode(ports::ByteRangeSource& session,
    domain::DownloadMode newMode)
{
    auto* http = dynamic_cast<HttpRangeAssetSession*>(&session);
    if (!http) {
        qCWarning(KINEMA_DOWNLOAD)
            << m_logName << "::changeMode on non-HTTP session";
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
        << m_logName << "::changeMode assetId=\"" << http->assetId()
        << "\" -> mode=" << static_cast<int>(newMode);
}

} // namespace kinema::playback::sources
