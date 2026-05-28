// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/MediaSourcePort.h"

namespace kinema::core {
class MediaCache;
}

namespace kinema::playback::torrent {
class LibtorrentClient;
}

namespace kinema::playback::sources {

/**
 * `MediaSourcePort` over the libtorrent-backed
 * `playback::torrent::LibtorrentClient`.
 *
 *   - `canHandle()` is true for any stream carrying an info hash.
 *   - `open()` calls `prepareSession()` on the engine in
 *     `Streaming` or `Background` mode depending on
 *     `DownloadMode`, sets the cache marker, builds a
 *     `playback::sources::TorrentAssetSession`, and applies
 *     keep-alive when the mode is `Full`.
 *   - `changeMode()` honours OnDemand→Full / Full→OnDemand by
 *     forwarding to the engine's `promoteToFull` /
 *     `setKeepAlive(false)` paths.
 *   - `filesFor()` returns the engine's file catalog (which is
 *     already typed as `domain::MediaFileEntry`).
 */
class TorrentMediaSource : public ports::MediaSourcePort
{
public:
    TorrentMediaSource(playback::torrent::LibtorrentClient& engine,
        core::MediaCache& cache);
    ~TorrentMediaSource() override;

    domain::DownloadBackendKind kind() const noexcept override
    {
        return domain::DownloadBackendKind::Torrent;
    }

    bool canHandle(const domain::Stream& s) const override;

    QCoro::Task<ports::OpenedSession> open(const domain::AssetRef& ref,
        const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadMode mode) override;

    void changeMode(ports::ByteRangeSource& session,
        domain::DownloadMode newMode) override;

    QVector<domain::MediaFileEntry> filesFor(
        const ports::ByteRangeSource& session) const override;

private:
    playback::torrent::LibtorrentClient& m_engine;
    core::MediaCache& m_cache;
};

} // namespace kinema::playback::sources
