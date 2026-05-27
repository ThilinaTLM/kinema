// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/ByteRangeSource.h"

#include <QCoro/QCoroTask>

#include <QString>
#include <QVector>

#include <memory>

namespace kinema::playback::ports {

/**
 * Open-session result from `MediaSourcePort::open`. Owns the
 * underlying byte-range session so the caller can register it on
 * `LocalHttpStreamGateway` and surface progress events.
 */
struct OpenedSession {
    QString assetId;
    std::unique_ptr<ByteRangeSource> session;
};

/**
 * Strategy interface for one media backend (torrent / debrid).
 *
 * Mirrors `download::DownloadBackend` but speaks in terms of the
 * playback subsystem's port types (`ByteRangeSource`,
 * `domain::MediaFileEntry`). Implementations live in
 * `playback::sources`.
 */
class MediaSourcePort
{
public:
    virtual ~MediaSourcePort() = default;

    /// Stable enum tag used by selection rules and UI.
    virtual domain::DownloadBackendKind kind() const noexcept = 0;

    /// True when this backend is configured and the stream has
    /// the affordances this backend needs.
    virtual bool canHandle(const domain::Stream& s) const = 0;

    /// Open a session for `ref` in the requested `mode`. The
    /// returned `OpenedSession.session` must implement
    /// `ByteRangeSource` and be ready to take requests.
    virtual QCoro::Task<OpenedSession> open(
        const domain::AssetRef& ref,
        const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadMode mode)
        = 0;

    /// Transition an already-opened session between OnDemand and
    /// Full mode. Optional; default is a no-op.
    virtual void changeMode(ByteRangeSource& /*session*/,
        domain::DownloadMode /*mode*/) {}

    /// File catalog for a previously-opened session. Defaults to
    /// empty so backends that only expose one file don't need to
    /// override.
    virtual QVector<domain::MediaFileEntry> filesFor(
        const ByteRangeSource& /*session*/) const
    {
        return {};
    }
};

} // namespace kinema::playback::ports
