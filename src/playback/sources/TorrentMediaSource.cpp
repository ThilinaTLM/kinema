// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/TorrentMediaSource.h"

#include "core/persistence/MediaCache.h"
#include "download/TorrentAssetSession.h"
#include "kinema_log_download.h"
#include "torrent/TorrentStreamingService.h"

#include <memory>
#include <utility>

namespace kinema::playback::sources {

TorrentMediaSource::TorrentMediaSource(
    kinema::torrent::TorrentStreamingService& engine,
    core::MediaCache& cache)
    : m_engine(engine)
    , m_cache(cache)
{
}

TorrentMediaSource::~TorrentMediaSource() = default;

bool TorrentMediaSource::canHandle(const domain::Stream& s) const
{
    return !s.infoHash.isEmpty();
}

QCoro::Task<ports::OpenedSession> TorrentMediaSource::open(
    const domain::AssetRef& ref, const domain::Stream& stream,
    const domain::PlaybackContext& ctx, domain::DownloadMode mode)
{
    const auto assetId = domain::assetIdFor(ref);
    m_cache.markActive(assetId);

    const auto prepareMode = mode == domain::DownloadMode::Full
        ? kinema::torrent::PrepareMode::Background
        : kinema::torrent::PrepareMode::Streaming;
    const auto prepared
        = co_await m_engine.prepareSession(stream, ctx, prepareMode);

    auto session = std::make_unique<download::TorrentAssetSession>(
        m_engine, assetId, prepared.token, prepared.fileName,
        prepared.fileSize, prepared.infoHash);
    session->setMode(mode);

    if (mode == domain::DownloadMode::Full) {
        m_engine.setKeepAlive(prepared.infoHash, true);
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "TorrentMediaSource::open assetId=\"" << assetId
        << "\" infoHash=\"" << prepared.infoHash
        << "\" mode=" << static_cast<int>(mode)
        << " fileSize=" << prepared.fileSize;

    ports::OpenedSession out;
    out.assetId = assetId;
    out.session = std::move(session);
    co_return out;
}

void TorrentMediaSource::changeMode(ports::ByteRangeSource& session,
    domain::DownloadMode newMode)
{
    auto* torrentSession
        = dynamic_cast<download::TorrentAssetSession*>(&session);
    if (!torrentSession) {
        qCWarning(KINEMA_DOWNLOAD)
            << "TorrentMediaSource::changeMode on non-torrent session";
        return;
    }
    if (torrentSession->mode() == newMode) {
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
        << "TorrentMediaSource::changeMode assetId=\""
        << torrentSession->assetId()
        << "\" -> mode=" << static_cast<int>(newMode);
}

QVector<domain::MediaFileEntry> TorrentMediaSource::filesFor(
    const ports::ByteRangeSource& session) const
{
    const auto* torrentSession
        = dynamic_cast<const download::TorrentAssetSession*>(&session);
    if (!torrentSession) {
        return {};
    }
    const auto raw = m_engine.filesForInfoHash(torrentSession->infoHash());
    QVector<domain::MediaFileEntry> out;
    out.reserve(raw.size());
    for (const auto& f : raw) {
        domain::MediaFileEntry e;
        e.index = f.index;
        e.path = f.path;
        e.size = f.size;
        e.playable = true;
        out.append(std::move(e));
    }
    return out;
}

} // namespace kinema::playback::sources
