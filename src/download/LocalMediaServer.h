// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QCoro/QCoroTask>

#include <QHash>
#include <QObject>
#include <QPointer>
#include <QTcpServer>
#include <QUrl>

class QTcpSocket;

namespace kinema::download {

class AssetSession;

/**
 * Generic localhost HTTP server that fronts every download backend.
 *
 * - Sessions register a `(token, AssetSession*)` mapping. The token
 *   is stable for the lifetime of the session.
 * - For each incoming request the server resolves the token, asks
 *   the session to `ensureRange(...)`, then reads bytes via
 *   `readRange(...)`.
 * - One server instance per process; the `DownloadManager` owns it.
 * - Replaces the torrent-only `torrent::LocalStreamServer`.
 */
class LocalMediaServer : public QObject
{
    Q_OBJECT
public:
    explicit LocalMediaServer(QObject* parent = nullptr);
    ~LocalMediaServer() override;

    /// Bind on `127.0.0.1:0`. Idempotent.
    bool listen();

    /// True once `listen()` has succeeded.
    bool isListening() const noexcept { return m_server.isListening(); }

    /// Map a token to a session. Caller retains ownership of the
    /// session and is responsible for calling `unregisterSession()`
    /// before destroying it.
    void registerSession(AssetSession* session);
    void unregisterSession(AssetSession* session);
    void unregisterToken(const QString& token);

    /// `http://127.0.0.1:<port>/stream/<token>/<percent-encoded-name>`.
    QUrl urlFor(AssetSession* session) const;

private Q_SLOTS:
    void acceptConnection();

private:
    QCoro::Task<void> serveSocket(QTcpSocket* socket);
    AssetSession* sessionForToken(const QString& token) const;

    QTcpServer m_server;
    QHash<QString, QPointer<AssetSession>> m_sessions;
};

} // namespace kinema::download
