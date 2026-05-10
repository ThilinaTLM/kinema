// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/HttpAssetSession.h"

#include "config/DownloadSettings.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "kinema_log_download.h"

#include <KLocalizedString>

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QNetworkRequest>
#include <QUuid>

#include <algorithm>

namespace kinema::download {

namespace {

constexpr qint64 kDefaultChunk = 4LL * 1024LL * 1024LL;

} // namespace

HttpAssetSession::HttpAssetSession(core::HttpClient& http,
    RealDebridResolver& resolver,
    const config::DownloadSettings& settings,
    api::AssetRef ref,
    QString assetId,
    QString localDir,
    QObject* parent)
    : AssetSession(parent)
    , m_http(http)
    , m_resolver(resolver)
    , m_settings(settings)
    , m_ref(std::move(ref))
    , m_assetId(std::move(assetId))
    , m_token(QUuid::createUuid().toString(QUuid::WithoutBraces))
    , m_localDir(std::move(localDir))
    , m_fileName(m_ref.fileNameHint.isEmpty()
              ? (m_ref.releaseName.isEmpty()
                        ? QStringLiteral("stream.mkv")
                        : m_ref.releaseName + QStringLiteral(".mkv"))
              : m_ref.fileNameHint)
    , m_fileSize(m_ref.sizeBytes.value_or(-1))
    , m_chunkSize(kDefaultChunk)
{
    QDir().mkpath(m_localDir);
    loadChunkMap();
}

HttpAssetSession::~HttpAssetSession()
{
    saveChunkMap();
}

void HttpAssetSession::setChunkSize(qint64 bytes)
{
    if (bytes >= 64 * 1024 && bytes <= 64LL * 1024 * 1024) {
        m_chunkSize = bytes;
    }
}

QString HttpAssetSession::payloadPath() const
{
    return QDir(m_localDir).absoluteFilePath(QStringLiteral("payload.bin"));
}

QString HttpAssetSession::chunkMapPath() const
{
    return QDir(m_localDir).absoluteFilePath(QStringLiteral("chunks.map"));
}

int HttpAssetSession::chunkIndexForByte(qint64 byte) const noexcept
{
    return static_cast<int>(byte / m_chunkSize);
}

qint64 HttpAssetSession::chunkStart(int idx) const noexcept
{
    return static_cast<qint64>(idx) * m_chunkSize;
}

qint64 HttpAssetSession::chunkEndExclusive(int idx) const noexcept
{
    return std::min(chunkStart(idx) + m_chunkSize, m_fileSize);
}

bool HttpAssetSession::isChunkAvailable(int idx) const
{
    if (idx < 0 || idx >= m_totalChunks) {
        return false;
    }
    return m_chunkAvailable.at(static_cast<size_t>(idx));
}

void HttpAssetSession::markChunkAvailable(int idx)
{
    if (idx < 0 || idx >= m_totalChunks) {
        return;
    }
    m_chunkAvailable[static_cast<size_t>(idx)] = true;
}

void HttpAssetSession::loadChunkMap()
{
    QFile f(chunkMapPath());
    if (!f.open(QIODevice::ReadOnly)) {
        return;
    }
    const auto blob = f.readAll();
    // Format: 8 bytes file size (little-endian) + 8 bytes chunk size
    // + bitmap (one bit per chunk, packed LSB-first). Old files that
    // pre-date a bigger fileSize discovery are silently dropped.
    if (blob.size() < 16) {
        return;
    }
    qint64 storedFileSize = 0;
    qint64 storedChunkSize = 0;
    memcpy(&storedFileSize, blob.constData(), 8);
    memcpy(&storedChunkSize, blob.constData() + 8, 8);

    if (storedChunkSize <= 0) {
        return;
    }
    m_chunkSize = storedChunkSize;
    if (storedFileSize > 0) {
        m_fileSize = storedFileSize;
    }
    if (m_fileSize > 0) {
        m_totalChunks = static_cast<int>(
            (m_fileSize + m_chunkSize - 1) / m_chunkSize);
        m_chunkAvailable.assign(static_cast<size_t>(m_totalChunks), false);

        const auto bitmap = blob.mid(16);
        for (int i = 0; i < m_totalChunks; ++i) {
            const int byteIdx = i / 8;
            const int bitIdx = i % 8;
            if (byteIdx >= bitmap.size()) {
                break;
            }
            if ((static_cast<unsigned char>(bitmap.at(byteIdx))
                    >> bitIdx) & 0x1) {
                m_chunkAvailable[static_cast<size_t>(i)] = true;
            }
        }
    }
}

void HttpAssetSession::saveChunkMap() const
{
    if (m_totalChunks <= 0) {
        return;
    }
    QFile f(chunkMapPath());
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        return;
    }
    QByteArray blob;
    blob.reserve(16 + (m_totalChunks + 7) / 8);
    blob.resize(16);
    memcpy(blob.data(), &m_fileSize, 8);
    memcpy(blob.data() + 8, &m_chunkSize, 8);

    QByteArray bitmap((m_totalChunks + 7) / 8, '\0');
    for (int i = 0; i < m_totalChunks; ++i) {
        if (m_chunkAvailable.at(static_cast<size_t>(i))) {
            bitmap.data()[i / 8] |= static_cast<char>(1u << (i % 8));
        }
    }
    blob.append(bitmap);
    f.write(blob);
}

void HttpAssetSession::ensureFileSizedToTotal()
{
    if (m_fileSize <= 0) {
        return;
    }
    QFile f(payloadPath());
    if (!f.open(QIODevice::ReadWrite)) {
        return;
    }
    if (f.size() < m_fileSize) {
        f.resize(m_fileSize);
    }
}

qint64 HttpAssetSession::cachedBytes() const
{
    qint64 total = 0;
    for (int i = 0; i < m_totalChunks; ++i) {
        if (m_chunkAvailable.at(static_cast<size_t>(i))) {
            total += chunkEndExclusive(i) - chunkStart(i);
        }
    }
    return total;
}

void HttpAssetSession::touch()
{
    // No-op here; the manager handles MediaCache.touch() based on
    // server activity through `LocalMediaServer`.
}

QCoro::Task<void> HttpAssetSession::ensureResolved()
{
    if (m_resolveInFlight || !m_upstream.isEmpty()) {
        co_return;
    }
    m_resolveInFlight = true;
    try {
        const auto resolved = co_await m_resolver.resolve(m_ref);
        m_upstream = resolved.downloadUrl;
        m_rdTorrentId = resolved.rdTorrentId;
        if (resolved.fileSize > 0
            && (m_fileSize <= 0 || resolved.fileSize != m_fileSize)) {
            m_fileSize = resolved.fileSize;
            m_totalChunks = static_cast<int>(
                (m_fileSize + m_chunkSize - 1) / m_chunkSize);
            m_chunkAvailable.assign(
                static_cast<size_t>(m_totalChunks), false);
            ensureFileSizedToTotal();
            saveChunkMap();
        }
        if (!resolved.fileName.isEmpty()) {
            m_fileName = resolved.fileName;
        }
        m_resolveInFlight = false;
    } catch (...) {
        m_resolveInFlight = false;
        throw;
    }
}

QCoro::Task<bool> HttpAssetSession::fetchChunk(int chunkIndex)
{
    co_await ensureResolved();
    if (m_upstream.isEmpty() || m_fileSize <= 0) {
        co_return false;
    }
    const qint64 start = chunkStart(chunkIndex);
    const qint64 endIncl = chunkEndExclusive(chunkIndex) - 1;
    if (start > endIncl) {
        co_return false;
    }

    QNetworkRequest req(m_upstream);
    const auto rangeHeader = QStringLiteral("bytes=%1-%2")
                                 .arg(start)
                                 .arg(endIncl)
                                 .toUtf8();
    req.setRawHeader("Range", rangeHeader);

    QByteArray body;
    bool needRetry = false;
    QString failReason;
    try {
        body = co_await m_http.get(req);
    } catch (const core::HttpError& e) {
        const int s = e.httpStatus();
        if (s == 401 || s == 403 || s == 410) {
            qCInfo(KINEMA_DOWNLOAD) << "HttpAssetSession: upstream expired ("
                           << s << "), re-resolving";
            needRetry = true;
        } else {
            failReason = e.message();
        }
    } catch (const std::exception& e2) {
        failReason = QString::fromUtf8(e2.what());
    }
    if (!failReason.isEmpty()) {
        Q_EMIT failed(failReason);
        co_return false;
    }
    if (needRetry) {
        m_upstream = QUrl();
        try {
            co_await ensureResolved();
            QNetworkRequest req2(m_upstream);
            req2.setRawHeader("Range", rangeHeader);
            body = co_await m_http.get(req2);
        } catch (const std::exception& e2) {
            Q_EMIT failed(QString::fromUtf8(e2.what()));
            co_return false;
        }
    }

    QFile f(payloadPath());
    if (!f.open(QIODevice::ReadWrite)) {
        co_return false;
    }
    if (!f.seek(start)) {
        co_return false;
    }
    f.write(body);
    f.flush();
    f.close();

    markChunkAvailable(chunkIndex);
    saveChunkMap();
    const qint64 cached = cachedBytes();
    Q_EMIT cachedBytesChanged(cached);

    // Synthesise a download rate as an EWMA over byte deltas. The
    // first sample seeds the baseline; later samples blend the new
    // instantaneous bytes-per-second at α = 0.4.
    const qint64 nowMs = QDateTime::currentMSecsSinceEpoch();
    if (m_lastSampleAtMsec > 0) {
        const qint64 dtMs = nowMs - m_lastSampleAtMsec;
        const qint64 dBytes = cached - m_lastSampleBytes;
        if (dtMs > 50 && dBytes >= 0) {
            const qint64 instBps = (dBytes * 1000) / dtMs;
            m_emaRateBps = (m_emaRateBps == 0)
                ? instBps
                : (instBps * 4 + m_emaRateBps * 6) / 10;
        }
    }
    m_lastSampleAtMsec = nowMs;
    m_lastSampleBytes = cached;

    int eta = -1;
    if (m_emaRateBps > 0 && m_fileSize > cached) {
        eta = static_cast<int>((m_fileSize - cached) / m_emaRateBps);
    }
    Q_EMIT liveStatsChanged(m_emaRateBps, /*peers=*/0, /*seeds=*/0, eta);

    if (chunkIndex == m_totalChunks - 1
        && std::all_of(m_chunkAvailable.begin(),
            m_chunkAvailable.end(), [](bool v) { return v; })) {
        Q_EMIT completed();
    }
    co_return true;
}

QCoro::Task<bool> HttpAssetSession::ensureChunk(int chunkIndex)
{
    if (chunkIndex < 0 || chunkIndex >= m_totalChunks) {
        co_return false;
    }
    if (isChunkAvailable(chunkIndex)) {
        co_return true;
    }
    co_return co_await fetchChunk(chunkIndex);
}

QCoro::Task<bool> HttpAssetSession::ensureRange(ByteRange range)
{
    if (!range.isValid()) {
        co_return false;
    }
    co_await ensureResolved();
    if (m_fileSize <= 0) {
        co_return false;
    }
    range.endInclusive = std::min(range.endInclusive, m_fileSize - 1);
    const int firstChunk = chunkIndexForByte(range.start);
    const int lastChunk = chunkIndexForByte(range.endInclusive);

    for (int i = firstChunk; i <= lastChunk; ++i) {
        if (!co_await ensureChunk(i)) {
            co_return false;
        }
    }
    co_return true;
}

QByteArray HttpAssetSession::readRange(ByteRange range) const
{
    if (!range.isValid() || m_fileSize <= 0) {
        return {};
    }
    QFile f(payloadPath());
    if (!f.open(QIODevice::ReadOnly)) {
        return {};
    }
    const qint64 endIncl = std::min(range.endInclusive, m_fileSize - 1);
    if (range.start > endIncl) {
        return {};
    }
    if (!f.seek(range.start)) {
        return {};
    }
    return f.read(endIncl - range.start + 1);
}

QCoro::Task<void> HttpAssetSession::prefetchAll()
{
    co_await ensureResolved();
    for (int i = 0; i < m_totalChunks; ++i) {
        if (isChunkAvailable(i)) {
            continue;
        }
        co_await ensureChunk(i);
    }
}

} // namespace kinema::download
