// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Types.h"
#include "core/TorrentioConfig.h"

#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}

namespace kinema::api {

/**
 * Client for Torrentio's streams endpoint:
 *
 *   GET /[config/]stream/{kind}/{streamId}.json
 *
 * `streamId` is an IMDB id for movies (ttXXXXXX), or ttXXXXXX:{S}:{E} for a
 * specific series episode.
 */
class TorrentioClient : public QObject
{
    Q_OBJECT
public:
    explicit TorrentioClient(core::HttpClient* http, QObject* parent = nullptr);

    void setBaseUrl(QUrl url);
    const QUrl& baseUrl() const noexcept { return m_baseUrl; }

    QCoro::Task<QList<Stream>> streams(MediaKind kind,
        QString streamId,
        core::torrentio::ConfigOptions opts = {});

private:
    core::HttpClient* m_http;
    QUrl m_baseUrl;
};

} // namespace kinema::api
