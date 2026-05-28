// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/ports/ByteRangeSource.h"

#include <QObject>
#include <QString>
#include <QVector>

#include <memory>

namespace kinema::playback::sources {
class AssetSession;
}

namespace kinema::playback::transfer {

/**
 * Concrete runtime session owned by `SessionRegistry`.
 *
 * Holds the metadata the transfer subsystem needs to reason about a
 * single active asset (id, ref, playback context, backend kind,
 * mode + disposition) alongside ownership of the underlying byte
 * source. The source is a `playback::sources::AssetSession`
 * produced by a `MediaSourcePort::open(...)` call; the
 * `TransferSession` re-publishes its progress signals so the
 * `TransferSupervisor` can subscribe without depending on the
 * concrete source class.
 *
 * Progress and live telemetry produced by the underlying source
 * are re-published as QObject signals so the
 * `TransferSupervisor` can subscribe without depending on a
 * specific source class.
 */
class TransferSession : public QObject
{
    Q_OBJECT
public:
    TransferSession(domain::AssetRef ref,
        domain::PlaybackContext ctx,
        domain::DownloadBackendKind backend,
        domain::DownloadMode mode,
        domain::CacheDisposition disposition,
        std::unique_ptr<sources::AssetSession> source,
        PlaybackSessionId playbackSessionId = {},
        QObject* parent = nullptr);
    ~TransferSession() override;

    TransferSession(const TransferSession&) = delete;
    TransferSession& operator=(const TransferSession&) = delete;

    /// Stable identity inside the registry (`domain::assetIdFor`).
    QString assetId() const;

    /// Lower-case hex info hash this session was opened for. Empty
    /// when the source has no info hash (e.g. direct URL).
    QString infoHash() const noexcept { return m_ref.infoHash; }

    const domain::AssetRef& ref() const noexcept { return m_ref; }
    const domain::PlaybackContext& context() const noexcept { return m_ctx; }
    domain::DownloadBackendKind backendKind() const noexcept { return m_backend; }
    domain::DownloadMode mode() const noexcept { return m_mode; }
    domain::CacheDisposition disposition() const noexcept { return m_disposition; }
    PlaybackSessionId playbackSessionId() const noexcept { return m_playbackSessionId; }
    void setPlaybackSessionId(PlaybackSessionId id) noexcept { m_playbackSessionId = id; }

    /// Update the in-memory mode + disposition. The caller is
    /// responsible for asking the backend to apply the change.
    void setMode(domain::DownloadMode m);
    void setDisposition(domain::CacheDisposition d);

    /// Borrowed pointer to the underlying byte-range source. Always
    /// non-null; ownership remains with the `TransferSession`.
    ports::ByteRangeSource* byteRangeSource() noexcept;

    /// Borrowed pointer to the concrete `sources::AssetSession`.
    /// Returns the same object as `byteRangeSource()` but typed for
    /// the call sites that reach into backend-specific behaviour
    /// (pause / resume / mode flag).
    sources::AssetSession* source() noexcept { return m_source.get(); }

    /// Convenience forwarders mirroring `AssetSession`'s lifecycle
    /// surface.
    qint64 cachedBytes() const;
    qint64 fileSize() const;
    QString fileName() const;
    void touch();
    void pause();
    void resume();

    /// Files inside the underlying source, lifted to
    /// `domain::MediaFileEntry`. Empty when the source hasn't
    /// resolved metadata yet.
    QVector<domain::MediaFileEntry> files() const;

Q_SIGNALS:
    void cachedBytesChanged(qint64 bytes);
    void completed();
    void failed(const QString& reason);
    void liveStatsChanged(qint64 ratePayloadBps,
        int peers,
        int seeds,
        int etaSeconds);
    void statusMessage(const QString& text, int timeoutMs);

private:
    domain::AssetRef m_ref;
    domain::PlaybackContext m_ctx;
    domain::DownloadBackendKind m_backend;
    domain::DownloadMode m_mode;
    domain::CacheDisposition m_disposition;
    PlaybackSessionId m_playbackSessionId;
    std::unique_ptr<sources::AssetSession> m_source;
};

} // namespace kinema::playback::transfer
