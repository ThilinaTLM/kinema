// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/TmdbClient.h"

#include "api/TmdbParse.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"

#include <KLocalizedString>

#include <QLocale>
#include <QNetworkRequest>

namespace kinema::api {

namespace {

QString defaultLanguage()
{
    // QLocale::name() returns e.g. "en_US". TMDB wants BCP-47 "en-US".
    auto n = QLocale::system().name();
    n.replace(QLatin1Char('_'), QLatin1Char('-'));
    return n.isEmpty() ? QStringLiteral("en-US") : n;
}

QString defaultRegion()
{
    // Qt 6: country() / countryToCode() were renamed to
    // territory() / territoryToCode(). Use the new names.
    const auto t = QLocale::system().territory();
    const auto code = QLocale::territoryToCode(t);
    return code.isEmpty() ? QStringLiteral("US") : code;
}

} // namespace

TmdbClient::TmdbClient(core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_baseUrl(QStringLiteral("https://api.themoviedb.org/3"))
    , m_language(defaultLanguage())
    , m_region(defaultRegion())
{
}

void TmdbClient::setBaseUrl(QUrl url)
{
    m_baseUrl = std::move(url);
}

void TmdbClient::setToken(QString token)
{
    m_token = std::move(token);
}

void TmdbClient::setLanguage(QString language)
{
    m_language = std::move(language);
}

void TmdbClient::setRegion(QString region)
{
    m_region = std::move(region);
}

QUrl TmdbClient::buildUrl(const QString& path,
    std::initializer_list<std::pair<QString, QString>> extra) const
{
    QUrl url = m_baseUrl;
    // m_baseUrl has path "/3"; append without stomping it.
    url.setPath(m_baseUrl.path() + path);

    QUrlQuery q;
    q.addQueryItem(QStringLiteral("language"), m_language);
    for (const auto& [k, v] : extra) {
        if (!v.isEmpty()) {
            q.addQueryItem(k, v);
        }
    }
    url.setQuery(q);
    return url;
}

QNetworkRequest TmdbClient::authed(const QUrl& url) const
{
    QNetworkRequest req(url);
    req.setRawHeader("Authorization",
        QByteArrayLiteral("Bearer ") + m_token.toUtf8());
    req.setRawHeader("Accept", "application/json");
    return req;
}

namespace {

core::HttpError notConfigured()
{
    return core::HttpError(core::HttpError::Kind::HttpStatus, 401,
        i18n("No TMDB token configured."));
}

} // namespace

QCoro::Task<QList<DiscoverItem>> TmdbClient::trending(MediaKind kind, bool weekly)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto kindSeg = kind == MediaKind::Movie
        ? QStringLiteral("movie")
        : QStringLiteral("tv");
    const auto window = weekly
        ? QStringLiteral("week")
        : QStringLiteral("day");
    const auto url = buildUrl(QStringLiteral("/trending/%1/%2").arg(kindSeg, window));
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseList(doc, kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::popular(MediaKind kind)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto path = kind == MediaKind::Movie
        ? QStringLiteral("/movie/popular")
        : QStringLiteral("/tv/popular");
    const auto url = buildUrl(path);
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseList(doc, kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::topRated(MediaKind kind)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto path = kind == MediaKind::Movie
        ? QStringLiteral("/movie/top_rated")
        : QStringLiteral("/tv/top_rated");
    const auto url = buildUrl(path);
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseList(doc, kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::nowPlayingMovies()
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto url = buildUrl(QStringLiteral("/movie/now_playing"),
        { { QStringLiteral("region"), m_region } });
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseList(doc, MediaKind::Movie);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::onTheAirSeries()
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto url = buildUrl(QStringLiteral("/tv/on_the_air"));
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseList(doc, MediaKind::Series);
}

QCoro::Task<QString> TmdbClient::imdbIdForTmdbMovie(int tmdbId)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto url = buildUrl(QStringLiteral("/movie/%1").arg(tmdbId),
        { { QStringLiteral("append_to_response"),
            QStringLiteral("external_ids") } });
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseMovieExternalIds(doc);
}

QCoro::Task<QString> TmdbClient::imdbIdForTmdbSeries(int tmdbId)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto url = buildUrl(
        QStringLiteral("/tv/%1/external_ids").arg(tmdbId));
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseSeriesExternalIds(doc);
}

QCoro::Task<std::pair<int, MediaKind>> TmdbClient::findByImdb(
    QString imdbId, MediaKind preferredKind)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto url = buildUrl(QStringLiteral("/find/%1").arg(imdbId),
        { { QStringLiteral("external_source"),
            QStringLiteral("imdb_id") } });
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseFindResult(doc, preferredKind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::recommendations(MediaKind kind, int tmdbId)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto prefix = kind == MediaKind::Movie
        ? QStringLiteral("/movie/")
        : QStringLiteral("/tv/");
    const auto url = buildUrl(
        prefix + QString::number(tmdbId) + QStringLiteral("/recommendations"));
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseList(doc, kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::similar(MediaKind kind, int tmdbId)
{
    if (!hasToken()) {
        throw notConfigured();
    }
    const auto prefix = kind == MediaKind::Movie
        ? QStringLiteral("/movie/")
        : QStringLiteral("/tv/");
    const auto url = buildUrl(
        prefix + QString::number(tmdbId) + QStringLiteral("/similar"));
    const auto doc = co_await m_http->getJson(authed(url));
    co_return tmdb::parseList(doc, kind);
}

QCoro::Task<void> TmdbClient::testAuth()
{
    if (!hasToken()) {
        throw notConfigured();
    }
    // /authentication is a no-argument endpoint that returns 200 with
    // { "success": true } when the bearer is valid. We ignore the body.
    const auto url = buildUrl(QStringLiteral("/authentication"));
    (void)co_await m_http->getJson(authed(url));
}

} // namespace kinema::api
