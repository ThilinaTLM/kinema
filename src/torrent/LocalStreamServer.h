// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QCoro/QCoroTask>

#include <QObject>
#include <QTcpServer>
#include <QUrl>

class QTcpSocket;

namespace kinema::torrent {

class TorrentStreamingService;

/**
 * LEGACY localhost server kept private to `TorrentStreamingService`
 * for the deprecated `prepare()` entry point. Production playback
 * goes through `download::LocalMediaServer`.
 */
class LocalStreamServer : public QObject
{
    Q_OBJECT
public:
    explicit LocalStreamServer(TorrentStreamingService& service,
        QObject* parent = nullptr);

    bool listen();
    QUrl urlForToken(const QString& token, const QString& fileName) const;

private Q_SLOTS:
    void acceptConnection();

private:
    QCoro::Task<void> serveSocket(QTcpSocket* socket);

    TorrentStreamingService& m_service;
    QTcpServer m_server;
};

} // namespace kinema::torrent
