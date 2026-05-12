// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/TorrentBackend.h"

#include "core/persistence/MediaCache.h"
#include "download/TorrentAssetSession.h"
#include "kinema_log_download.h"
#include "torrent/TorrentStreamingService.h"

namespace kinema::download {

TorrentBackend::TorrentBackend(torrent::TorrentStreamingService& engine,
    core::MediaCache& cache)
    : m_engine(engine)
    , m_cache(cache)
{
}

TorrentBackend::~TorrentBackend() = default;

bool TorrentBackend::canHandle(const domain::Stream& s) const
{
    return !s.infoHash.isEmpty();
}

QCoro::Task<std::unique_ptr<AssetSession>> TorrentBackend::open(
    const domain::AssetRef& ref,
    const domain::Stream& s,
    const domain::PlaybackContext& ctx,
    domain::DownloadMode mode)
{
    const auto assetId = domain::assetIdFor(ref);
    m_cache.markActive(assetId);

    const auto prepareMode = mode == domain::DownloadMode::Full
        ? torrent::PrepareMode::Background
        : torrent::PrepareMode::Streaming;
    const auto prepared = co_await m_engine.prepareSession(s, ctx, prepareMode);

    auto session = std::make_unique<TorrentAssetSession>(m_engine,
        assetId, prepared.token, prepared.fileName, prepared.fileSize,
        prepared.infoHash);
    session->setMode(mode);

    if (mode == domain::DownloadMode::Full) {
        m_engine.setKeepAlive(prepared.infoHash, true);
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "TorrentBackend::open assetId=\"" << assetId
        << "\" infoHash=\"" << prepared.infoHash
        << "\" mode=" << static_cast<int>(mode)
        << " fileSize=" << prepared.fileSize;

    co_return session;
}

void TorrentBackend::changeMode(AssetSession& session,
    domain::DownloadMode newMode)
{
    if (session.mode() == newMode) {
        return;
    }
    auto* torrentSession = qobject_cast<TorrentAssetSession*>(&session);
    if (!torrentSession) {
        qCWarning(KINEMA_DOWNLOAD)
            << "TorrentBackend::changeMode called on non-torrent session";
        return;
    }
    const auto& hash = torrentSession->infoHash();
    if (newMode == domain::DownloadMode::Full) {
        m_engine.promoteToFull(hash);
    } else {
        // Demotion (Full -> OnDemand) just clears keepAlive; we
        // intentionally don't try to re-warm the streaming buffer
        // because demotion is rare and the player (if attached)
        // will pull pieces it needs via `ensureRange()` anyway.
        m_engine.setKeepAlive(hash, false);
    }
    torrentSession->setMode(newMode);
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "TorrentBackend::changeMode assetId=\"" << session.assetId()
        << "\" -> mode=" << static_cast<int>(newMode);
}

} // namespace kinema::download
