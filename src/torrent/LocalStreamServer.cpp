// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "torrent/LocalStreamServer.h"

#include "torrent/TorrentStreamingService.h"

#include <QCoro/QCoroIODevice>
#include <QCoro/QCoroSignal>

#include <QFileInfo>
#include <QHostAddress>
#include <QPointer>
#include <QTcpSocket>
#include <QUrlQuery>
#include <QStringList>

#include <optional>

namespace kinema::torrent {

namespace {
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

void writeHeaders(QTcpSocket* socket, int status, qint64 contentLength,
    const QByteArray& contentType = QByteArrayLiteral("application/octet-stream"),
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

QString tokenFromPath(const QString& path)
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

std::optional<HttpRequest> parseRequest(const QByteArray& raw,
    TorrentStreamingService& service)
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
    const auto token = tokenFromPath(req.path);
    const auto size = service.fileSizeForToken(token);
    if (size > 0) {
        req.range = parseRangeHeader(lines, size);
    }
    return req;
}
}

LocalStreamServer::LocalStreamServer(TorrentStreamingService& service,
    QObject* parent)
    : QObject(parent)
    , m_service(service)
{
    connect(&m_server, &QTcpServer::newConnection,
        this, &LocalStreamServer::acceptConnection);
}

bool LocalStreamServer::listen()
{
    if (m_server.isListening()) {
        return true;
    }
    return m_server.listen(QHostAddress::LocalHost, 0);
}

QUrl LocalStreamServer::urlForToken(const QString& token,
    const QString& fileName) const
{
    QUrl url;
    url.setScheme(QStringLiteral("http"));
    url.setHost(QStringLiteral("127.0.0.1"));
    url.setPort(m_server.serverPort());
    url.setPath(QStringLiteral("/stream/%1/%2").arg(token,
        QString::fromUtf8(QUrl::toPercentEncoding(QFileInfo(fileName).fileName()))));
    return url;
}

void LocalStreamServer::acceptConnection()
{
    while (auto* socket = m_server.nextPendingConnection()) {
        socket->setParent(this);
        auto task = serveSocket(socket);
        Q_UNUSED(task);
    }
}

QCoro::Task<void> LocalStreamServer::serveSocket(QTcpSocket* socket)
{
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

    const auto req = parseRequest(raw, m_service);
    if (!req) {
        writeHeaders(guard, 400, 0, {});
        guard->disconnectFromHost();
        co_return;
    }

    const QString token = tokenFromPath(req->path);
    const qint64 fileSize = m_service.fileSizeForToken(token);
    if (token.isEmpty() || fileSize <= 0) {
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    m_service.touchToken(token);

    ByteRange range = req->range.value_or(ByteRange { 0, fileSize - 1 });
    if (!range.isValid() || range.start >= fileSize) {
        QByteArray extra = "Content-Range: bytes */" + QByteArray::number(fileSize) + "\r\n";
        writeHeaders(guard, 416, 0, {}, extra);
        guard->disconnectFromHost();
        co_return;
    }
    range.endInclusive = qMin(range.endInclusive, fileSize - 1);

    const bool partial = req->range.has_value();
    const qint64 length = range.endInclusive - range.start + 1;
    QByteArray extra;
    if (partial) {
        extra = "Content-Range: bytes " + QByteArray::number(range.start)
            + "-" + QByteArray::number(range.endInclusive)
            + "/" + QByteArray::number(fileSize) + "\r\n";
    }
    writeHeaders(guard, partial ? 206 : 200, length,
        QByteArrayLiteral("video/x-matroska"), extra);

    if (req->method.compare(QStringLiteral("HEAD"), Qt::CaseInsensitive) == 0) {
        guard->disconnectFromHost();
        co_return;
    }
    if (req->method.compare(QStringLiteral("GET"), Qt::CaseInsensitive) != 0) {
        guard->disconnectFromHost();
        co_return;
    }

    const bool ready = co_await m_service.ensureRange(token, range);
    if (!guard || !ready) {
        if (guard) {
            guard->disconnectFromHost();
        }
        co_return;
    }

    const QByteArray body = m_service.readRange(token, range);
    if (guard) {
        guard->write(body);
        co_await qCoro(guard.data(), &QTcpSocket::bytesWritten);
        guard->disconnectFromHost();
    }
}

} // namespace kinema::torrent
