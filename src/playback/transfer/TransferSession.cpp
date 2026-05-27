// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/transfer/TransferSession.h"

#include "playback/sources/AssetSession.h"

#include <utility>

namespace kinema::playback::transfer {

TransferSession::TransferSession(domain::AssetRef ref,
    domain::PlaybackContext ctx,
    domain::DownloadBackendKind backend,
    domain::DownloadMode mode,
    domain::CacheDisposition disposition,
    std::unique_ptr<sources::AssetSession> source,
    QObject* parent)
    : QObject(parent)
    , m_ref(std::move(ref))
    , m_ctx(std::move(ctx))
    , m_backend(backend)
    , m_mode(mode)
    , m_disposition(disposition)
    , m_source(std::move(source))
{
    Q_ASSERT(m_source);
    if (!m_source) {
        return;
    }
    // Keep the legacy session's mode flag aligned with ours so any
    // direct `AssetSession::mode()` reads stay coherent during the
    // transitional period.
    m_source->setMode(m_mode);

    // Re-publish progress / telemetry so subscribers can attach to
    // the `TransferSession` without depending on the concrete
    // backend session class.
    connect(m_source.get(), &sources::AssetSession::cachedBytesChanged,
        this, &TransferSession::cachedBytesChanged);
    connect(m_source.get(), &sources::AssetSession::completed,
        this, &TransferSession::completed);
    connect(m_source.get(), &sources::AssetSession::failed,
        this, &TransferSession::failed);
    connect(m_source.get(), &sources::AssetSession::liveStatsChanged,
        this, &TransferSession::liveStatsChanged);
    connect(m_source.get(), &sources::AssetSession::statusMessage,
        this, &TransferSession::statusMessage);
}

TransferSession::~TransferSession() = default;

QString TransferSession::assetId() const
{
    return m_source ? m_source->assetId() : domain::assetIdFor(m_ref);
}

void TransferSession::setMode(domain::DownloadMode m)
{
    m_mode = m;
    if (m_source) {
        m_source->setMode(m);
    }
}

void TransferSession::setDisposition(domain::CacheDisposition d)
{
    m_disposition = d;
}

ports::ByteRangeSource* TransferSession::byteRangeSource() noexcept
{
    return m_source.get();
}

qint64 TransferSession::cachedBytes() const
{
    return m_source ? m_source->cachedBytes() : -1;
}

qint64 TransferSession::fileSize() const
{
    return m_source ? m_source->fileSize() : -1;
}

QString TransferSession::fileName() const
{
    return m_source ? m_source->fileName() : QString();
}

void TransferSession::touch()
{
    if (m_source) {
        m_source->touch();
    }
}

void TransferSession::pause()
{
    if (m_source) {
        m_source->pause();
    }
}

void TransferSession::resume()
{
    if (m_source) {
        m_source->resume();
    }
}

QVector<domain::MediaFileEntry> TransferSession::files() const
{
    if (!m_source) {
        return {};
    }
    const auto raw = m_source->files();
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

} // namespace kinema::playback::transfer
