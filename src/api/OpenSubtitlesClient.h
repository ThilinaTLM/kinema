// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Subtitle.h"

#include <QNetworkRequest>
#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}

namespace kinema::api {

/**
 * Client for the OpenSubtitles.com REST v1 API.
 *
 * Required headers on every authenticated call:
 *   - Api-Key:       user's key
 *   - Authorization: Bearer JWT     (download + authenticated search)
 *   - Accept:        wildcard       (their CDN is picky)
 *   - User-Agent:    Kinema/<ver>   (set by HttpClient)
 *
 * Token aliases come from `controllers::TokenController` so live
 * updates flow without re-construction. JWT is held in memory only;
 * on 401 we clear it and re-login once.
 */
class OpenSubtitlesClient : public QObject
{
    Q_OBJECT
public:
    OpenSubtitlesClient(core::HttpClient* http,
        const QString& apiKeyAlias,
        const QString& usernameAlias,
        const QString& passwordAlias,
        QObject* parent = nullptr);

    /// Override the base URL (default `https://api.opensubtitles.com/api/v1`).
    void setBaseUrl(QUrl url);

    /// True iff a JWT is currently cached. Flips back to false on 401.
    bool isAuthenticated() const noexcept { return !m_jwt.isEmpty(); }

    /// True iff all three credentials (api key + username + password)
    /// are non-empty. Driven by the token aliases.
    bool hasCredentials() const noexcept;

    /// Force-clear the cached JWT. Call after credentials change so
    /// the next call performs a fresh login.
    void clearJwt();

    virtual QCoro::Task<QList<SubtitleHit>> search(SubtitleSearchQuery q);

    /// No-op when a JWT is already cached. Throws on credential
    /// failure.
    virtual QCoro::Task<void> ensureLoggedIn();

    /// Hit `/download` for `fileId`. Auto-retries once on 401 after a
    /// fresh login.
    virtual QCoro::Task<SubtitleDownloadTicket> requestDownload(QString fileId);

    /// Plain GET on the signed link (no auth header). Follows
    /// redirects.
    virtual QCoro::Task<QByteArray> fetchFileBytes(QUrl link);

private:
    QNetworkRequest baseRequest(const QUrl& url, bool authed) const;
    QUrl buildUrl(const QString& path) const;

    core::HttpClient* m_http;
    const QString& m_apiKey;
    const QString& m_username;
    const QString& m_password;
    QUrl m_baseUrl;
    QString m_jwt;
};

} // namespace kinema::api
