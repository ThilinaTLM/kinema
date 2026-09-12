// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/AllDebrid.h"

#include <QList>
#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}

namespace kinema::api {
using namespace kinema::domain;

/**
 * Client for the AllDebrid REST v4 / v4.1 API.
 *
 * AllDebrid plays the same role as Real-Debrid in Kinema's unified
 * downloader. The resolution flow used by `AllDebridResolver` is:
 *
 *   POST /v4/magnet/upload
 *   POST /v4.1/magnet/status   (poll until statusCode == 4)
 *   POST /v4/magnet/files      (flatten the tree, pick the best file)
 *   POST /v4/link/unlock       (\u00b1 polling /v4/link/delayed)
 *   POST /v4/magnet/delete     (best-effort cleanup)
 *
 * Auth: every request carries `Authorization: Bearer <apikey>`.
 *
 * Responses are wrapped in `{ status, data | error }`; the parsing
 * layer (`api::alldebrid::*`) unwraps the envelope and translates
 * `status:"error"` into `core::HttpError`.
 */
class AllDebridClient : public QObject
{
    Q_OBJECT
public:
    explicit AllDebridClient(core::HttpClient* http,
        QObject* parent = nullptr);

    /// Override the AllDebrid base URL (default
    /// https://api.alldebrid.com). Tests use this to point at a
    /// local fake.
    void setBaseUrl(QUrl url);
    const QUrl& baseUrl() const noexcept { return m_baseUrl; }

    /// In-memory only. The apikey is supplied by the caller per-session.
    void setApiKey(QString apiKey);
    const QString& apiKey() const noexcept { return m_apiKey; }

    /// Throws HttpError. Auth failures surface as HttpStatus 401.
    virtual QCoro::Task<AllDebridUser> user();

    /// Upload one magnet. The URI may be a full magnet: URI or just
    /// a 40-char info hash; AllDebrid accepts both.
    virtual QCoro::Task<AdAddMagnetResult> uploadMagnet(QString magnetUri);

    /// Get the status of a single magnet by id. Uses /v4.1/.
    virtual QCoro::Task<AdMagnetStatus> magnetStatus(qint64 magnetId);

    /// Get the file tree for a magnet, flattened into leaf files.
    virtual QCoro::Task<QList<AdMagnetFile>> magnetFiles(qint64 magnetId);

    /// Unlock a hoster link. When the response is "delayed" this
    /// transparently polls /v4/link/delayed every 5s up to a
    /// 90s ceiling and returns the final URL.
    virtual QCoro::Task<AdUnlockedLink> unlockLink(QUrl link);

    /// Delete a magnet from the user's account. Best-effort:
    /// callers swallow errors on cleanup.
    virtual QCoro::Task<void> deleteMagnet(qint64 magnetId);

private:
    QCoro::Task<AdUnlockedLink> pollDelayed(qint64 delayedId);

    core::HttpClient* m_http;
    QUrl m_baseUrl;
    QString m_apiKey;
};

} // namespace kinema::api
