// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "torrent/TorrentStreamingService.h"

#include "kinema_log_torrent.h"
#include "torrent/LocalStreamServer.h"

#include <KLocalizedString>

#include <stdexcept>

namespace kinema::torrent {

namespace {

std::runtime_error runtimeError(const QString& msg)
{
    return std::runtime_error(msg.toStdString());
}

} // namespace

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

TorrentStreamingService::TorrentStreamingService(core::TorrentCache& cache,
    const config::TorrentStreamingSettings& settings, QObject* parent)
    : QObject(parent)
    , m_client(std::make_unique<kinema::playback::torrent::LibtorrentClient>(
          settings, cache, this))
{
    // Forward the client's typed signals through TSS's existing
    // signal surface so legacy subscribers (ShellViewModel,
    // TorrentAssetSession) keep working unchanged.
    connect(m_client.get(),
        &kinema::playback::torrent::LibtorrentClient::statusMessage,
        this,
        [this](const QString& text, int timeoutMs) {
            Q_EMIT statusMessage(text, timeoutMs);
        });
    connect(m_client.get(),
        &kinema::playback::torrent::LibtorrentClient::statsUpdated,
        this,
        [this](const QString& hash, qint64 done, qint64 rate,
            int peers, int seeds, int eta, bool /*finished*/) {
            Q_EMIT torrentStatsChanged(hash, done, rate, peers, seeds, eta);
        });
    connect(m_client.get(),
        &kinema::playback::torrent::LibtorrentClient::torrentFinished,
        this, &TorrentStreamingService::torrentFinished);
    connect(m_client.get(),
        &kinema::playback::torrent::LibtorrentClient::torrentFailed,
        this, &TorrentStreamingService::torrentFailed);

    qCDebug(KINEMA_TORRENT)
        << "TorrentStreamingService constructed (dormant)";
}

TorrentStreamingService::TorrentStreamingService(StubTag, QObject* parent)
    : QObject(parent)
{
    // Stub instances have no backing services; every method on the
    // base class no-ops when `m_client` is null so test doubles
    // stay completely dormant.
}

TorrentStreamingService::~TorrentStreamingService() = default;

// ---------------------------------------------------------------------------
// Forwarders
// ---------------------------------------------------------------------------

QCoro::Task<PreparedSession> TorrentStreamingService::prepareSession(
    const domain::Stream& stream, const domain::PlaybackContext& ctx,
    PrepareMode mode)
{
    if (!m_client) {
        throw runtimeError(i18nc("@info:status",
            "Torrent streaming is not available in this build."));
    }
    co_return co_await m_client->prepareSession(stream, ctx, mode);
}

QCoro::Task<QUrl> TorrentStreamingService::prepare(
    const domain::Stream& stream, const domain::PlaybackContext& ctx)
{
    if (!m_client) {
        throw runtimeError(i18nc("@info:status",
            "Torrent streaming is not available in this build."));
    }
    if (!m_legacyServer) {
        m_legacyServer = std::make_unique<LocalStreamServer>(*this, this);
    }
    if (!m_legacyServer->listen()) {
        throw runtimeError(i18nc("@info:status",
            "Could not start the local torrent streaming server."));
    }
    const auto ps = co_await m_client->prepareSession(stream, ctx,
        PrepareMode::Streaming);
    co_return m_legacyServer->urlForToken(ps.token, ps.fileName);
}

QCoro::Task<bool> TorrentStreamingService::ensureRange(const QString& token,
    ByteRange range)
{
    if (!m_client) {
        co_return false;
    }
    co_return co_await m_client->ensureRange(token, range);
}

QByteArray TorrentStreamingService::readRange(const QString& token,
    ByteRange range) const
{
    return m_client ? m_client->readRange(token, range) : QByteArray {};
}

qint64 TorrentStreamingService::fileSizeForToken(const QString& token) const
{
    return m_client ? m_client->fileSizeForToken(token) : 0;
}

QString TorrentStreamingService::fileNameForToken(const QString& token) const
{
    return m_client ? m_client->fileNameForToken(token) : QString {};
}

void TorrentStreamingService::touchToken(const QString& token)
{
    if (m_client) {
        m_client->touchToken(token);
    }
}

QVector<TorrentFileEntry> TorrentStreamingService::filesForInfoHash(
    const QString& infoHash) const
{
    return m_client ? m_client->filesForInfoHash(infoHash)
                    : QVector<TorrentFileEntry> {};
}

void TorrentStreamingService::setKeepAlive(const QString& infoHash, bool on)
{
    if (m_client) {
        m_client->setKeepAlive(infoHash, on);
    }
}

void TorrentStreamingService::pauseInfoHash(const QString& infoHash)
{
    if (m_client) {
        m_client->pauseInfoHash(infoHash);
    }
}

void TorrentStreamingService::resumeInfoHash(const QString& infoHash)
{
    if (m_client) {
        m_client->resumeInfoHash(infoHash);
    }
}

void TorrentStreamingService::promoteToFull(const QString& infoHash)
{
    if (m_client) {
        m_client->promoteToFull(infoHash);
    }
}

void TorrentStreamingService::stopInfoHash(const QString& infoHash)
{
    if (m_client) {
        m_client->stopInfoHash(infoHash);
    }
}

void TorrentStreamingService::stopForContext(const domain::PlaybackContext& ctx)
{
    if (m_client) {
        m_client->stopForContext(ctx);
    }
}

void TorrentStreamingService::stopAll()
{
    if (m_client) {
        m_client->stopAll();
    }
}

} // namespace kinema::torrent
