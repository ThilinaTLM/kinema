// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/streaming/LocalHttpStreamGateway.h"

#include "kinema_log_download.h"

#include <QElapsedTimer>
#include <QFileInfo>
#include <QHostAddress>
#include <QObject>
#include <QPointer>
#include <QStringList>
#include <QTcpSocket>
#include <QUrlQuery>

#include <QCoro/QCoroIODevice>
#include <QCoro/QCoroSignal>

#include <algorithm>
#include <optional>

namespace kinema::playback::streaming {

namespace {

using kinema::core::ByteRange;
using kinema::playback::ports::ByteRangeSource;

struct HttpRequest
{
    QString method;
    QString path;
    std::optional<ByteRange> range;
};

QByteArray reason(int status)
{
    switch (status) {
    case 200:
        return QByteArrayLiteral("OK");
    case 206:
        return QByteArrayLiteral("Partial Content");
    case 400:
        return QByteArrayLiteral("Bad Request");
    case 404:
        return QByteArrayLiteral("Not Found");
    case 416:
        return QByteArrayLiteral("Range Not Satisfiable");
    case 500:
        return QByteArrayLiteral("Internal Server Error");
    default:
        return QByteArrayLiteral("Error");
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

void writeHeaders(QTcpSocket* socket,
                  int status,
                  qint64 contentLength,
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

std::optional<ByteRange> parseRangeHeader(const QList<QByteArray>& lines, qint64 fileSize)
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
            return ByteRange{1, 0};
        }
        if (!okEnd || end >= fileSize) {
            end = fileSize - 1;
        }
        return ByteRange{start, end};
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

LocalHttpStreamGateway::LocalHttpStreamGateway(QObject* parent) : QObject(parent)
{
    connect(&m_server, &QTcpServer::newConnection, this, &LocalHttpStreamGateway::acceptConnection);
}

LocalHttpStreamGateway::~LocalHttpStreamGateway() = default;

bool LocalHttpStreamGateway::listen()
{
    if (m_server.isListening()) {
        return true;
    }
    return m_server.listen(QHostAddress::LocalHost, 0);
}

QUrl LocalHttpStreamGateway::expose(ByteRangeSource& source)
{
    m_sources.insert(source.assetId(), &source);
    return buildUrl(source.assetId(), source.fileName());
}

QUrl LocalHttpStreamGateway::urlFor(const QString& assetId) const
{
    const auto it = m_sources.constFind(assetId);
    if (it == m_sources.constEnd() || !it.value()) {
        return {};
    }
    return buildUrl(assetId, it.value()->fileName());
}

void LocalHttpStreamGateway::revoke(ByteRangeSource& source)
{
    const auto id = source.assetId();
    const auto it = m_sources.constFind(id);
    if (it != m_sources.constEnd() && it.value() == &source) {
        m_sources.erase(it);
    }
}

void LocalHttpStreamGateway::revoke(const QString& assetId)
{
    m_sources.remove(assetId);
}

void LocalHttpStreamGateway::setSessionResolver(SessionResolver resolver)
{
    m_resolver = std::move(resolver);
}

LocalHttpStreamGateway::ByteRangeSource*
LocalHttpStreamGateway::sourceForAssetId(const QString& assetId) const
{
    const auto it = m_sources.constFind(assetId);
    if (it == m_sources.constEnd()) {
        return nullptr;
    }
    return it.value();
}

QCoro::Task<LocalHttpStreamGateway::ByteRangeSource*>
LocalHttpStreamGateway::ensureSourceForAssetId(const QString& assetId)
{
    if (auto* source = sourceForAssetId(assetId)) {
        co_return source;
    }
    if (!m_resolver || assetId.isEmpty()) {
        co_return nullptr;
    }
    co_return co_await m_resolver(assetId);
}

QUrl LocalHttpStreamGateway::buildUrl(const QString& assetId, const QString& fileName) const
{
    QUrl url;
    url.setScheme(QStringLiteral("http"));
    url.setHost(QStringLiteral("127.0.0.1"));
    url.setPort(m_server.serverPort());
    // Pass the bare filename to QUrl::setPath; QUrl percent-encodes
    // reserved characters once on toEncoded(). Pre-encoding here
    // would double-encode (e.g. "%20" -> "%2520").
    url.setPath(QStringLiteral("/stream/%1/%2").arg(assetId, QFileInfo(fileName).fileName()));
    return url;
}

void LocalHttpStreamGateway::acceptConnection()
{
    while (auto* socket = m_server.nextPendingConnection()) {
        socket->setParent(this);
        auto task = serveSocket(socket);
        Q_UNUSED(task);
    }
}

QCoro::Task<void> LocalHttpStreamGateway::serveSocket(QTcpSocket* socket)
{
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
        qCDebug(KINEMA_DOWNLOAD) << "LocalHttpStreamGateway: 400 — could not parse request line";
        writeHeaders(guard, 400, 0, {});
        guard->disconnectFromHost();
        co_return;
    }

    const QString assetId = assetIdFromPath(req->path);
    auto* source = co_await ensureSourceForAssetId(assetId);
    if (assetId.isEmpty() || !source) {
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "LocalHttpStreamGateway: 404 " << req->method << " path=\"" << req->path
            << "\" assetId=\"" << assetId << "\" (no live source)";
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    auto* sourceObject = dynamic_cast<QObject*>(source);
    QPointer<QObject> sourceGuard(sourceObject);
    const auto sourceAlive = [&sourceGuard, sourceObject]() {
        return sourceObject == nullptr || !sourceGuard.isNull();
    };

    const qint64 fileSize = source->fileSize();
    if (!sourceAlive()) {
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    if (fileSize <= 0) {
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "LocalHttpStreamGateway: 404 " << req->method << " assetId=\"" << assetId
            << "\" (fileSize=" << fileSize << ")";
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    source->touch();

    const auto rangeOpt = parseRangeHeader(raw.split('\n'), fileSize);
    ByteRange range = rangeOpt.value_or(ByteRange{0, fileSize - 1});
    if (!range.isValid() || range.start >= fileSize) {
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "LocalHttpStreamGateway: 416 " << req->method << " assetId=\"" << assetId
            << "\" range=" << range.start << "-" << range.endInclusive << " fileSize=" << fileSize;
        QByteArray extra = "Content-Range: bytes */" + QByteArray::number(fileSize) + "\r\n";
        writeHeaders(guard, 416, 0, {}, extra);
        guard->disconnectFromHost();
        co_return;
    }
    range.endInclusive = qMin(range.endInclusive, fileSize - 1);

    const bool partial = rangeOpt.has_value();
    const qint64 length = range.endInclusive - range.start + 1;
    const QByteArray ct = contentTypeFor(source->fileName());
    QByteArray extra;
    if (partial) {
        extra = "Content-Range: bytes " + QByteArray::number(range.start) + "-"
                + QByteArray::number(range.endInclusive) + "/" + QByteArray::number(fileSize)
                + "\r\n";
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "LocalHttpStreamGateway: " << (partial ? 206 : 200) << " " << req->method
        << " assetId=\"" << assetId << "\" range=" << range.start << "-" << range.endInclusive
        << " length=" << length << " fileSize=" << fileSize;
    writeHeaders(guard, partial ? 206 : 200, length, ct, extra);

    if (req->method.compare(QStringLiteral("HEAD"), Qt::CaseInsensitive) == 0) {
        guard->disconnectFromHost();
        co_return;
    }
    if (req->method.compare(QStringLiteral("GET"), Qt::CaseInsensitive) != 0) {
        qCDebug(KINEMA_DOWNLOAD).nospace()
            << "LocalHttpStreamGateway: ignoring unsupported method " << req->method;
        guard->disconnectFromHost();
        co_return;
    }

    qint64 cursor = range.start;
    qint64 totalSent = 0;
    bool ensureFailed = false;
    while (cursor <= range.endInclusive && guard) {
        const qint64 chunkEnd = std::min(cursor + kStreamChunkBytes - 1, range.endInclusive);
        const ByteRange chunk{cursor, chunkEnd};

        if (!sourceAlive()) {
            ensureFailed = true;
            break;
        }
        const bool ready = co_await source->ensureRange(chunk);
        if (!guard) {
            break;
        }
        if (!sourceAlive()) {
            ensureFailed = true;
            break;
        }
        if (!ready) {
            ensureFailed = true;
            break;
        }
        const QByteArray body = source->readRange(chunk);
        if (body.isEmpty()) {
            ensureFailed = true;
            break;
        }
        guard->write(body);
        while (guard && guard->bytesToWrite() > kStreamChunkBytes) {
            co_await qCoro(guard.data(), &QTcpSocket::bytesWritten);
        }
        cursor = chunkEnd + 1;
        totalSent += body.size();
    }

    if (!guard) {
        qCDebug(KINEMA_DOWNLOAD).nospace()
            << "LocalHttpStreamGateway: client disconnected mid-stream assetId=\"" << assetId
            << "\" sent=" << totalSent << "/" << length << " bytes";
        co_return;
    }
    if (ensureFailed) {
        qCWarning(KINEMA_DOWNLOAD).nospace()
            << "LocalHttpStreamGateway: ensureRange/readRange failed assetId=\"" << assetId
            << "\" cursor=" << cursor << " sent=" << totalSent << "/" << length;
        guard->disconnectFromHost();
        co_return;
    }

    co_await qCoro(guard.data(), &QTcpSocket::bytesWritten);
    qCDebug(KINEMA_DOWNLOAD).nospace()
        << "LocalHttpStreamGateway: complete assetId=\"" << assetId << "\" sent=" << totalSent
        << "/" << length << " elapsedMs=" << elapsed.elapsed();
    guard->disconnectFromHost();
}

} // namespace kinema::playback::streaming
