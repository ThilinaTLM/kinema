// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}

namespace kinema::api {

/**
 * Client for Cinemeta — the metadata source used by Stremio and mirrored
 * here so Kinema's metadata quality matches Stremio's.
 *
 * Endpoints:
 *   GET /catalog/{kind}/top/search={query}.json  → search
 *   GET /meta/{kind}/{imdbId}.json               → meta detail
 */
class CinemetaClient : public QObject
{
    Q_OBJECT
public:
    explicit CinemetaClient(core::HttpClient* http, QObject* parent = nullptr);

    /// Override the Cinemeta base URL (default: https://v3-cinemeta.strem.io).
    void setBaseUrl(QUrl url);
    const QUrl& baseUrl() const noexcept { return m_baseUrl; }

    QCoro::Task<QList<MetaSummary>> search(MediaKind kind, QString query);
    QCoro::Task<MetaDetail> meta(MediaKind kind, QString imdbId);

    /// Series meta + episode list in one call. The body is the same
    /// endpoint as `meta(Series, id)` but we parse the `videos` array too.
    QCoro::Task<SeriesDetail> seriesMeta(QString imdbId);

private:
    core::HttpClient* m_http;
    QUrl m_baseUrl;
};

} // namespace kinema::api
