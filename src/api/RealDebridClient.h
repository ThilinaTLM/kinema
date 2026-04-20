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
 * Client for the Real-Debrid REST v1 API. Kinema only ever calls the
 * /user endpoint — it's used to validate the token the user pastes into
 * the Real-Debrid dialog. Actual torrent resolution still goes through
 * Torrentio (which includes the token in its per-stream config).
 *
 * Endpoint:
 *   GET /rest/1.0/user   with  Authorization: Bearer <token>
 */
class RealDebridClient : public QObject
{
    Q_OBJECT
public:
    explicit RealDebridClient(core::HttpClient* http, QObject* parent = nullptr);

    /// Override the RD base URL (default: https://api.real-debrid.com/rest/1.0).
    void setBaseUrl(QUrl url);
    const QUrl& baseUrl() const noexcept { return m_baseUrl; }

    /// In-memory only. The token is supplied by the caller per-session.
    void setToken(QString token);
    const QString& token() const noexcept { return m_token; }

    /// Throws HttpError. Auth failures surface as HttpStatus 401.
    QCoro::Task<RealDebridUser> user();

private:
    core::HttpClient* m_http;
    QUrl m_baseUrl;
    QString m_token;
};

} // namespace kinema::api
