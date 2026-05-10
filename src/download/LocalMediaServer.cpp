// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/LocalMediaServer.h"

#include "download/AssetSession.h"

#include <QCoro/QCoroIODevice>
#include <QCoro/QCoroSignal>

#include <QFileInfo>
#include <QHostAddress>
#include <QPointer>
#include <QStringList>
#include <QTcpSocket>
#include <QUrlQuery>

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
    url.setPath(QStringLiteral("/stream/%1/%2").arg(
        session->assetId(),
        QString::fromUtf8(QUrl::toPercentEncoding(
            QFileInfo(session->fileName()).fileName()))));
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
        writeHeaders(guard, 400, 0, {});
        guard->disconnectFromHost();
        co_return;
    }

    const QString assetId = assetIdFromPath(req->path);
    auto* session = co_await ensureSessionForAssetId(assetId);
    if (assetId.isEmpty() || !session) {
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    const qint64 fileSize = session->fileSize();
    if (fileSize <= 0) {
        writeHeaders(guard, 404, 0, {});
        guard->disconnectFromHost();
        co_return;
    }
    session->touch();

    const auto rangeOpt = parseRangeHeader(raw.split('\n'), fileSize);
    ByteRange range = rangeOpt.value_or(ByteRange { 0, fileSize - 1 });
    if (!range.isValid() || range.start >= fileSize) {
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
    writeHeaders(guard, partial ? 206 : 200, length, ct, extra);

    if (req->method.compare(QStringLiteral("HEAD"), Qt::CaseInsensitive) == 0) {
        guard->disconnectFromHost();
        co_return;
    }
    if (req->method.compare(QStringLiteral("GET"), Qt::CaseInsensitive) != 0) {
        guard->disconnectFromHost();
        co_return;
    }

    const bool ready = co_await session->ensureRange(range);
    if (!guard || !ready) {
        if (guard) {
            guard->disconnectFromHost();
        }
        co_return;
    }

    const QByteArray body = session->readRange(range);
    if (guard) {
        guard->write(body);
        co_await qCoro(guard.data(), &QTcpSocket::bytesWritten);
        guard->disconnectFromHost();
    }
}

} // namespace kinema::download
