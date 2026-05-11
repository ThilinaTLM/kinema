// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/LocalMediaServer.h"

#include "download/AssetSession.h"
#include "kinema_log_download.h"

#include <QCoro/QCoroIODevice>
#include <QCoro/QCoroSignal>

#include <QElapsedTimer>
#include <QFileInfo>
#include <QHostAddress>
#include <QPointer>
#include <QStringList>
#include <QTcpSocket>
#include <QUrlQuery>

#include <algorithm>
#include <optional>

namespace kinema::download {

namespace {

using torrent::ByteRange;

struct HttpRequest {
    QString method;
    QString path;
    std::optional<ByteRange> range;
};

QByteArray reason(int status)
{
    switch (status) {
    case 200: return QByteArrayLiteral("OK");
    case 206: return QByteArrayLiteral("Partial Content");
    case 400: return QByteArrayLiteral("Bad Request");
    case 404: return QByteArrayLiteral("Not Found");
    case 416: return QByteArrayLiteral("Range Not Satisfiable");
    case 500: return QByteArrayLiteral("Internal Server Error");
    default: return QByteArrayLiteral("Error");
    }
}

QByteArray contentTypeFor(const QString& fileName)
{
    const auto suffix = QFileInfo(fileName).suffix().toLower();
    if (suffix == QLatin1String("mkv")) {
        return QByteArrayLiteral("video/x-matroska");
    }
    if (suffix == QLatin1String("mp4") || suffix == QLatin1String("m4v")) {
        return QByteArrayLiteral("video/mp4");
    }
    if (suffix == QLatin1String("webm")) {
        return QByteArrayLiteral("video/webm");
    }
    if (suffix == QLatin1String("avi")) {
        return QByteArrayLiteral("video/x-msvideo");
    }
    if (suffix == QLatin1String("mov")) {
        return QByteArrayLiteral("video/quicktime");
    }
    if (suffix == QLatin1String("ts")) {
        return QByteArrayLiteral("video/mp2t");
    }
    return QByteArrayLiteral("application/octet-stream");
}

void writeHeaders(QTcpSocket* socket, int status, qint64 contentLength,
    const QByteArray& contentType,
    const QByteArray& extra = {})
{
    QByteArray h;
    h += "HTTP/1.1 " + QByteArray::number(status) + " " + reason(status) + "\r\n";
    h += "Accept-Ranges: bytes\r\n";
    h += "Connection: close\r\n";
    if (!contentType.isEmpty()) {
        h += "Content-Type: " + contentType + "\r\n";
    }
    h += "Content-Length: " + QByteArray::number(qMax<qint64>(0, contentLength)) + "\r\n";
    h += extra;
    h += "\r\n";
    socket->write(h);
}

QString assetIdFromPath(const QString& path)
{
    const QStringList parts = path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    if (parts.size() < 2 || parts.at(0) != QLatin1String("stream")) {
        return {};
    }
    return parts.at(1);
}

std::optional<ByteRange> parseRangeHeader(const QList<QByteArray>& lines,
    qint64 fileSize)
{
    for (const auto& line : lines) {
        if (!line.toLower().startsWith("range:")) {
            continue;
        }
        const QByteArray v = line.mid(line.indexOf(':') + 1).trimmed();
        if (!v.startsWith("bytes=")) {
            return std::nullopt;
        }
        const auto spec = v.mid(6);
        const int dash = spec.indexOf('-');
        if (dash < 0) {
            return std::nullopt;
        }
        bool okStart = false;
        bool okEnd = false;
        const qint64 start = spec.left(dash).toLongLong(&okStart);
        qint64 end = spec.mid(dash + 1).toLongLong(&okEnd);
        if (!okStart || start < 0 || start >= fileSize) {
            return ByteRange { 1, 0 };
        }
        if (!okEnd || end >= fileSize) {
            end = fileSize - 1;
        }
        return ByteRange { start, end };
    }
    return std::nullopt;
}

std::optional<HttpRequest> parseRequestLine(const QByteArray& raw)
{
    const QList<QByteArray> lines = raw.split('\n');
    if (lines.isEmpty()) {
        return std::nullopt;
    }
    const QList<QByteArray> first = lines.first().trimmed().split(' ');
    if (first.size() < 2) {
        return std::nullopt;
    }
    HttpRequest req;
    req.method = QString::fromLatin1(first.at(0));
    req.path = QUrl::fromPercentEncoding(first.at(1));
    return req;
}

} // namespace

LocalMediaServer::LocalMediaServer(QObject* parent)
    : QObject(parent)
{
    connect(&m_server, &QTcpServer::newConnection,
        this, &LocalMediaServer::acceptConnection);
}

LocalMediaServer::~LocalMediaServer() = default;

bool LocalMediaServer::listen()
{
    if (m_server.isListening()) {
        return true;
    }
    return m_server.listen(QHostAddress::LocalHost, 0);
}

void LocalMediaServer::registerSession(AssetSession* session)
{
    if (!session) {
        return;
    }
    m_sessions.insert(session->assetId(), QPointer<AssetSession>(session));
}

void LocalMediaServer::unregisterSession(AssetSession* session)
{
    if (!session) {
        return;
    }
    m_sessions.remove(session->assetId());
}

void LocalMediaServer::unregisterAssetId(const QString& assetId)
{
    m_sessions.remove(assetId);
}

void LocalMediaServer::setSessionResolver(SessionResolver resolver)
{
    m_resolver = std::move(resolver);
}

AssetSession* LocalMediaServer::sessionForAssetId(const QString& assetId) const
{
    const auto it = m_sessions.constFind(assetId);
    if (it == m_sessions.constEnd()) {
        return nullptr;
    }
    return it.value().data();
}

QCoro::Task<AssetSession*> LocalMediaServer::ensureSessionForAssetId(
    const QString& assetId)
{
    if (auto* session = sessionForAssetId(assetId)) {
        co_return session;
    }
    if (!m_resolver || assetId.isEmpty()) {
        co_return nullptr;
    }
    co_return co_await m_resolver(assetId);
}

QUrl LocalMediaServer::urlFor(AssetSession* session) const
{
    if (!session) {
        return {};
    }
    QUrl url;
    url.setScheme(QStringLiteral("http"));
    url.setHost(QStringLiteral("127.0.0.1"));
    url.setPort(m_server.serverPort());
    // Pass the bare filename to QUrl::setPath; QUrl percent-encodes
    // reserved characters once on toEncoded(). Pre-encoding here
    // would double-encode (e.g. "%20" -> "%2520"), which mpv
    // tolerates but pollutes logs and breaks curl-based diagnostics.
    // Asset ids contain only [0-9a-f-]+ digits/letters so they need
    // no encoding either.
    url.setPath(QStringLiteral("/stream/%1/%2").arg(
        session->assetId(),
        QFileInfo(session->fileName()).fileName()));
    return url;
}

void LocalMediaServer::acceptConnection()
{
    while (auto* socket = m_server.nextPendingConnection()) {
        socket->setParent(this);
        auto task = serveSocket(socket);
        Q_UNUSED(task);
    }
}

QCoro::Task<void> LocalMediaServer::serveSocket(QTcpSocket* socket)
{
    // 1 MiB streaming window. mpv normally issues sub-megabyte
    // Range GETs so this only changes behaviour for non-Range
    // clients (curl without -r, generic external players, our own
    // diagnostic scripts), but it also caps peak memory per
    // request and lets ensureRange make progress incrementally
    // for backends that fetch lazily.
    constexpr qint64 kStreamChunkBytes = 1LL * 1024 * 1024;

    QElapsedTimer elapsed;
    elapsed.start();

    QPointer<QTcpSocket> guard(socket);
    if (!guard->canReadLine()) {
        co_await qCoro(guard.data(), &QTcpSocket::readyRead);
    }
    if (!guard) {
        co_return;
    }

    QByteArray raw;
    while (guard) {
        raw += guard->readAll();
        if (raw.contains("\r\n\r\n") || raw.contains("\n\n")) {
            break;
        }
        co_await qCoro(guard.data(), &QTcpSocket::readyRead);
    }
    if (!guard) {
        co_return;
    }

    const auto req = parseRequestLine(raw);
    if (!req) {
        qCDebug(KINEMA_DOWNLOAD)
            << "LocalMediaServer: 400 — could not parse request line";
        writeHeaders(guard, 400, 0, {});
        guard->disconnectFromHost();
        co_return;
    }

    const QString assetId = assetIdFromPath(req->path);
    auto* session = co_await ensureSessionForAssetId(assetId);
    if (assetId.isEmpty() || !session) {
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "LocalMediaServer: 404 " << req->method
            << " path=\"" << req->path
            << "\" assetId=\"" << assetId << "\" (no live session)";
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    const qint64 fileSize = session->fileSize();
    if (fileSize <= 0) {
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "LocalMediaServer: 404 " << req->method
            << " assetId=\"" << assetId
            << "\" (fileSize=" << fileSize << ")";
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    session->touch();

    const auto rangeOpt = parseRangeHeader(raw.split('\n'), fileSize);
    ByteRange range = rangeOpt.value_or(ByteRange { 0, fileSize - 1 });
    if (!range.isValid() || range.start >= fileSize) {
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "LocalMediaServer: 416 " << req->method
            << " assetId=\"" << assetId
            << "\" range=" << range.start << "-" << range.endInclusive
            << " fileSize=" << fileSize;
        QByteArray extra = "Content-Range: bytes */" + QByteArray::number(fileSize) + "\r\n";
        writeHeaders(guard, 416, 0, {}, extra);
        guard->disconnectFromHost();
        co_return;
    }
    range.endInclusive = qMin(range.endInclusive, fileSize - 1);

    const bool partial = rangeOpt.has_value();
    const qint64 length = range.endInclusive - range.start + 1;
    const QByteArray ct = contentTypeFor(session->fileName());
    QByteArray extra;
    if (partial) {
        extra = "Content-Range: bytes " + QByteArray::number(range.start)
            + "-" + QByteArray::number(range.endInclusive)
            + "/" + QByteArray::number(fileSize) + "\r\n";
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "LocalMediaServer: " << (partial ? 206 : 200) << " " << req->method
        << " assetId=\"" << assetId
        << "\" range=" << range.start << "-" << range.endInclusive
        << " length=" << length << " fileSize=" << fileSize;
    writeHeaders(guard, partial ? 206 : 200, length, ct, extra);

    if (req->method.compare(QStringLiteral("HEAD"), Qt::CaseInsensitive) == 0) {
        guard->disconnectFromHost();
        co_return;
    }
    if (req->method.compare(QStringLiteral("GET"), Qt::CaseInsensitive) != 0) {
        qCDebug(KINEMA_DOWNLOAD).nospace()
            << "LocalMediaServer: ignoring unsupported method "
            << req->method;
        guard->disconnectFromHost();
        co_return;
    }

    // Stream the body chunk by chunk so:
    //  - non-Range GETs do not have to await ensureRange() on the
    //    full file before any bytes flow,
    //  - peak memory per request is bounded by kStreamChunkBytes,
    //  - a slow consumer can apply backpressure via bytesWritten.
    qint64 cursor = range.start;
    qint64 totalSent = 0;
    bool ensureFailed = false;
    while (cursor <= range.endInclusive && guard) {
        const qint64 chunkEnd = std::min(
            cursor + kStreamChunkBytes - 1, range.endInclusive);
        const ByteRange chunk { cursor, chunkEnd };

        const bool ready = co_await session->ensureRange(chunk);
        if (!guard) {
            break;
        }
        if (!ready) {
            ensureFailed = true;
            break;
        }
        const QByteArray body = session->readRange(chunk);
        if (body.isEmpty()) {
            ensureFailed = true;
            break;
        }
        guard->write(body);
        // Backpressure: don't queue another chunk while the kernel
        // / Qt buffer is still draining the previous one.
        while (guard && guard->bytesToWrite() > kStreamChunkBytes) {
            co_await qCoro(guard.data(), &QTcpSocket::bytesWritten);
        }
        cursor = chunkEnd + 1;
        totalSent += body.size();
    }

    if (!guard) {
        qCDebug(KINEMA_DOWNLOAD).nospace()
            << "LocalMediaServer: client disconnected mid-stream assetId=\""
            << assetId << "\" sent=" << totalSent
            << "/" << length << " bytes";
        co_return;
    }
    if (ensureFailed) {
        qCWarning(KINEMA_DOWNLOAD).nospace()
            << "LocalMediaServer: ensureRange/readRange failed assetId=\""
            << assetId << "\" cursor=" << cursor
            << " sent=" << totalSent << "/" << length;
        guard->disconnectFromHost();
        co_return;
    }

    co_await qCoro(guard.data(), &QTcpSocket::bytesWritten);
    qCDebug(KINEMA_DOWNLOAD).nospace()
        << "LocalMediaServer: complete assetId=\"" << assetId
        << "\" sent=" << totalSent << "/" << length
        << " elapsedMs=" << elapsed.elapsed();
    guard->disconnectFromHost();
}

} // namespace kinema::download
