// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "torrent/PiecePlanner.h"

#include <QCoro/QCoroTask>

#include <QObject>
#include <QUrl>

#include <memory>

namespace kinema::config {
class TorrentStreamingSettings;
}

namespace kinema::core {
class TorrentCache;
}

namespace kinema::torrent {

class LocalStreamServer;

class TorrentStreamingService : public QObject
{
    Q_OBJECT
public:
    TorrentStreamingService(core::TorrentCache& cache,
        const config::TorrentStreamingSettings& settings,
        QObject* parent = nullptr);
    ~TorrentStreamingService() override;

    virtual QCoro::Task<QUrl> prepare(const api::Stream& stream,
        const api::PlaybackContext& ctx);

    QCoro::Task<bool> ensureRange(const QString& token, ByteRange range);
    QByteArray readRange(const QString& token, ByteRange range) const;
    qint64 fileSizeForToken(const QString& token) const;
    QString fileNameForToken(const QString& token) const;
    void touchToken(const QString& token);

public Q_SLOTS:
    void stopInfoHash(const QString& infoHash);
    void stopForContext(const api::PlaybackContext& ctx);
    void stopAll();

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    struct Private;
    std::unique_ptr<Private> d;
};

} // namespace kinema::torrent
