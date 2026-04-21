// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

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

    // NB: these are virtual so controller tests can subclass
    // CinemetaClient with in-memory fakes. Production callers pay
    // one vtable dispatch per network call — noise compared to the
    // HTTP round-trip.
    virtual QCoro::Task<QList<MetaSummary>> search(MediaKind kind, QString query);
    virtual QCoro::Task<MetaDetail> meta(MediaKind kind, QString imdbId);

    /// Series meta + episode list in one call. The body is the same
    /// endpoint as `meta(Series, id)` but we parse the `videos` array too.
    virtual QCoro::Task<SeriesDetail> seriesMeta(QString imdbId);

    virtual ~CinemetaClient() = default;

private:
    core::HttpClient* m_http;
    QUrl m_baseUrl;
};

} // namespace kinema::api
