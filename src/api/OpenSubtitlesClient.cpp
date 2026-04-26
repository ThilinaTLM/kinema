// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/OpenSubtitlesClient.h"

#include "api/OpenSubtitlesParse.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "kinema_debug.h"

#include <KLocalizedString>

#include <QJsonDocument>
#include <QJsonObject>
#include <QUrlQuery>

namespace kinema::api {

namespace {

QString stripTtPrefix(const QString& imdb)
{
    if (imdb.startsWith(QLatin1String("tt"))) {
        return imdb.mid(2);
    }
    return imdb;
}

core::HttpError missingCredentials()
{
    return core::HttpError(core::HttpError::Kind::HttpStatus, 401,
        i18n("OpenSubtitles credentials not configured."));
}

} // namespace

OpenSubtitlesClient::OpenSubtitlesClient(core::HttpClient* http,
    const QString& apiKeyAlias,
    const QString& usernameAlias,
    const QString& passwordAlias,
    QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_apiKey(apiKeyAlias)
    , m_username(usernameAlias)
    , m_password(passwordAlias)
    , m_baseUrl(QStringLiteral("https://api.opensubtitles.com/api/v1"))
{
}

void OpenSubtitlesClient::setBaseUrl(QUrl url)
{
    m_baseUrl = std::move(url);
}

bool OpenSubtitlesClient::hasCredentials() const noexcept
{
    return !m_apiKey.isEmpty() && !m_username.isEmpty()
        && !m_password.isEmpty();
}

void OpenSubtitlesClient::clearJwt()
{
    m_jwt.clear();
}

QUrl OpenSubtitlesClient::buildUrl(const QString& path) const
{
    QUrl url = m_baseUrl;
    url.setPath(m_baseUrl.path() + path);
    return url;
}

QNetworkRequest OpenSubtitlesClient::baseRequest(const QUrl& url, bool authed) const
{
    QNetworkRequest req(url);
    req.setRawHeader("Api-Key", m_apiKey.toUtf8());
    req.setRawHeader("Accept", "*/*");
    if (authed && !m_jwt.isEmpty()) {
        req.setRawHeader("Authorization",
            QByteArrayLiteral("Bearer ") + m_jwt.toUtf8());
    }
    return req;
}

QCoro::Task<QList<SubtitleHit>> OpenSubtitlesClient::search(
    SubtitleSearchQuery q)
{
    if (!hasCredentials()) {
        throw missingCredentials();
    }
    if (!q.key.isValid()) {
        throw core::HttpError(core::HttpError::Kind::HttpStatus, 400,
            i18n("Subtitle search requires an IMDb id."));
    }

    QUrl url = buildUrl(QStringLiteral("/subtitles"));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("imdb_id"), stripTtPrefix(q.key.imdbId));
    if (q.key.season.has_value()) {
        query.addQueryItem(QStringLiteral("season_number"),
            QString::number(*q.key.season));
    }
    if (q.key.episode.has_value()) {
        query.addQueryItem(QStringLiteral("episode_number"),
            QString::number(*q.key.episode));
    }
    if (!q.languages.isEmpty()) {
        query.addQueryItem(QStringLiteral("languages"),
            q.languages.join(QLatin1Char(',')));
    }
    if (!q.hearingImpaired.isEmpty()
        && q.hearingImpaired != QLatin1String("off")) {
        query.addQueryItem(QStringLiteral("hearing_impaired"),
            q.hearingImpaired);
    }
    if (!q.foreignPartsOnly.isEmpty()
        && q.foreignPartsOnly != QLatin1String("off")) {
        query.addQueryItem(QStringLiteral("foreign_parts_only"),
            q.foreignPartsOnly);
    }
    if (!q.moviehash.isEmpty()) {
        query.addQueryItem(QStringLiteral("moviehash"), q.moviehash);
    }
    query.addQueryItem(QStringLiteral("per_page"), QStringLiteral("50"));
    url.setQuery(query);

    // The search endpoint accepts either anonymous (with just Api-Key)
    // or authenticated requests. We send the JWT when we have one so
    // the response includes quota fields, but we don't *require* it.
    auto req = baseRequest(url, /*authed=*/true);
    const auto doc = co_await m_http->getJson(std::move(req));
    co_return opensubtitles::parseSearch(doc);
}

QCoro::Task<void> OpenSubtitlesClient::ensureLoggedIn()
{
    if (!hasCredentials()) {
        throw missingCredentials();
    }
    if (!m_jwt.isEmpty()) {
        co_return;
    }
    QJsonObject body;
    body[QStringLiteral("username")] = m_username;
    body[QStringLiteral("password")] = m_password;
    const QByteArray bytes = QJsonDocument(body).toJson(QJsonDocument::Compact);

    auto req = baseRequest(buildUrl(QStringLiteral("/login")),
        /*authed=*/false);
    const auto doc = co_await m_http->postJsonForJson(std::move(req), bytes);
    m_jwt = opensubtitles::parseLogin(doc);
    qCDebug(KINEMA) << "OpenSubtitles login OK";
}

QCoro::Task<SubtitleDownloadTicket> OpenSubtitlesClient::requestDownload(
    QString fileId)
{
    if (!hasCredentials()) {
        throw missingCredentials();
    }
    if (fileId.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::HttpStatus, 400,
            i18n("Missing subtitle file id."));
    }

    co_await ensureLoggedIn();

    QJsonObject body;
    body[QStringLiteral("file_id")] = fileId.toLongLong();
    const QByteArray bytes = QJsonDocument(body).toJson(QJsonDocument::Compact);

    auto req = baseRequest(buildUrl(QStringLiteral("/download")),
        /*authed=*/true);

    bool retry = false;
    try {
        const auto doc = co_await m_http->postJsonForJson(std::move(req), bytes);
        co_return opensubtitles::parseDownload(doc);
    } catch (const core::HttpError& e) {
        if (e.httpStatus() != 401) {
            throw;
        }
        retry = true;
    }

    // JWT expired or revoked — clear and try once more.
    qCDebug(KINEMA) << "OpenSubtitles 401 on /download \u2014 refreshing JWT";
    m_jwt.clear();
    co_await ensureLoggedIn();
    auto retryReq = baseRequest(buildUrl(QStringLiteral("/download")),
        /*authed=*/true);
    const auto doc = co_await m_http->postJsonForJson(
        std::move(retryReq), bytes);
    Q_UNUSED(retry);
    co_return opensubtitles::parseDownload(doc);
}

QCoro::Task<QByteArray> OpenSubtitlesClient::fetchFileBytes(QUrl link)
{
    QNetworkRequest req(link);
    co_return co_await m_http->get(std::move(req));
}

} // namespace kinema::api
