// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/ports/ByteRangeSource.h"

#include <QCoro/QCoroTask>

#include <QHash>
#include <QObject>
#include <QPointer>
#include <QTcpServer>
#include <QUrl>

#include <functional>

class QTcpSocket;

namespace kinema::playback::streaming {

/**
 * Generic localhost HTTP server that fronts every media source.
 *
 * - Sources expose themselves via `expose(ByteRangeSource&)`, which
 *   registers the source by `assetId()` and returns the localhost
 *   URL the player should `loadfile`.
 * - For each incoming request the gateway resolves the `assetId`,
 *   asks the source to `ensureRange(...)`, then reads bytes via
 *   `readRange(...)`.
 * - When no live source exists, the gateway may ask an injected
 *   resolver callback to materialise one from persisted state.
 * - One gateway instance per process; the composition root owns it.
 */
class LocalHttpStreamGateway : public QObject
{
    Q_OBJECT
public:
    using ByteRangeSource = kinema::playback::ports::ByteRangeSource;
    using SessionResolver = std::function<QCoro::Task<ByteRangeSource*>(
        const QString& assetId)>;

    explicit LocalHttpStreamGateway(QObject* parent = nullptr);
    ~LocalHttpStreamGateway() override;

    /// Bind on `127.0.0.1:0`. Idempotent.
    bool listen();

    /// True once `listen()` has succeeded.
    bool isListening() const noexcept { return m_server.isListening(); }

    /// Expose `source` on the gateway. Returns the localhost URL
    /// the player should load. Caller retains ownership and must
    /// `revoke(assetId)` before destroying the source.
    QUrl expose(ByteRangeSource& source);

    /// Look up a previously-exposed source by asset id and return
    /// its URL, or an invalid `QUrl` if the source is no longer
    /// registered.
    QUrl urlFor(const QString& assetId) const;

    /// Remove the registration for `source` (resolved by `assetId()`).
    void revoke(ByteRangeSource& source);

    /// Remove the registration for `assetId` regardless of which
    /// source previously occupied that slot.
    void revoke(const QString& assetId);

    void setSessionResolver(SessionResolver resolver);

private Q_SLOTS:
    void acceptConnection();

private:
    QCoro::Task<void> serveSocket(QTcpSocket* socket);
    QCoro::Task<ByteRangeSource*> ensureSourceForAssetId(
        const QString& assetId);
    ByteRangeSource* sourceForAssetId(const QString& assetId) const;
    QUrl buildUrl(const QString& assetId, const QString& fileName) const;

    QTcpServer m_server;
    QHash<QString, ByteRangeSource*> m_sources;
    SessionResolver m_resolver;
};

} // namespace kinema::playback::streaming
