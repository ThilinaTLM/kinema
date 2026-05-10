// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QCoro/QCoroTask>

#include <QHash>
#include <QObject>
#include <QPointer>
#include <QTcpServer>
#include <QUrl>

#include <functional>

class QTcpSocket;

namespace kinema::download {

class AssetSession;

/**
 * Generic localhost HTTP server that fronts every download backend.
 *
 * - Sessions register an `(assetId, AssetSession*)` mapping.
 * - For each incoming request the server resolves the `assetId`,
 *   asks the session to `ensureRange(...)`, then reads bytes via
 *   `readRange(...)`.
 * - When no live session exists, the server may ask an injected
 *   resolver callback to re-realise one from persisted state.
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

    /// Map an asset id to a session. Caller retains ownership of the
    /// session and is responsible for calling `unregisterSession()`
    /// before destroying it.
    void registerSession(AssetSession* session);
    void unregisterSession(AssetSession* session);
    void unregisterAssetId(const QString& assetId);

    using SessionResolver = std::function<QCoro::Task<AssetSession*>(const QString& assetId)>;
    void setSessionResolver(SessionResolver resolver);

    /// `http://127.0.0.1:<port>/stream/<assetId>/<percent-encoded-name>`.
    QUrl urlFor(AssetSession* session) const;

private Q_SLOTS:
    void acceptConnection();

private:
    QCoro::Task<void> serveSocket(QTcpSocket* socket);
    QCoro::Task<AssetSession*> ensureSessionForAssetId(const QString& assetId);
    AssetSession* sessionForAssetId(const QString& assetId) const;

    QTcpServer m_server;
    QHash<QString, QPointer<AssetSession>> m_sessions;
    SessionResolver m_resolver;
};

} // namespace kinema::download
