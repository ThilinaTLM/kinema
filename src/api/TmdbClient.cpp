// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TmdbClient.h"

#include "api/TmdbDiscoverUrl.h"
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
    if (m_language == language) {
        return;
    }
    m_language = std::move(language);
    // Genre names are localised on the server, so the cache is no
    // longer valid under the new language.
    m_genresMovie.clear();
    m_genresSeries.clear();
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

QUrl TmdbClient::buildUrl(const QString& path, const QUrlQuery& extra) const
{
    QUrl url = m_baseUrl;
    url.setPath(m_baseUrl.path() + path);

    QUrlQuery q;
    q.addQueryItem(QStringLiteral("language"), m_language);
    const auto items = extra.queryItems();
    for (const auto& [k, v] : items) {
        q.addQueryItem(k, v);
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

QString kindMovieOrTv(MediaKind kind)
{
    return kind == MediaKind::Movie
        ? QStringLiteral("movie")
        : QStringLiteral("tv");
}

} // namespace

void TmdbClient::requireToken() const
{
    if (!hasToken()) {
        throw notConfigured();
    }
}

QCoro::Task<QJsonDocument> TmdbClient::fetch(QUrl url)
{
    co_return co_await m_http->getJson(authed(url));
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::trending(MediaKind kind, bool weekly)
{
    requireToken();
    const auto window = weekly
        ? QStringLiteral("week")
        : QStringLiteral("day");
    const auto url = buildUrl(
        QStringLiteral("/trending/%1/%2").arg(kindMovieOrTv(kind), window));
    co_return tmdb::parseList(co_await fetch(url), kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::popular(MediaKind kind)
{
    requireToken();
    const auto url = buildUrl(
        QStringLiteral("/%1/popular").arg(kindMovieOrTv(kind)));
    co_return tmdb::parseList(co_await fetch(url), kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::topRated(MediaKind kind)
{
    requireToken();
    const auto url = buildUrl(
        QStringLiteral("/%1/top_rated").arg(kindMovieOrTv(kind)));
    co_return tmdb::parseList(co_await fetch(url), kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::nowPlayingMovies()
{
    requireToken();
    const auto url = buildUrl(QStringLiteral("/movie/now_playing"),
        { { QStringLiteral("region"), m_region } });
    co_return tmdb::parseList(co_await fetch(url), MediaKind::Movie);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::onTheAirSeries()
{
    requireToken();
    const auto url = buildUrl(QStringLiteral("/tv/on_the_air"));
    co_return tmdb::parseList(co_await fetch(url), MediaKind::Series);
}

QCoro::Task<QString> TmdbClient::imdbIdForTmdbMovie(int tmdbId)
{
    requireToken();
    const auto url = buildUrl(QStringLiteral("/movie/%1").arg(tmdbId),
        { { QStringLiteral("append_to_response"),
            QStringLiteral("external_ids") } });
    co_return tmdb::parseMovieExternalIds(co_await fetch(url));
}

QCoro::Task<QString> TmdbClient::imdbIdForTmdbSeries(int tmdbId)
{
    requireToken();
    const auto url = buildUrl(
        QStringLiteral("/tv/%1/external_ids").arg(tmdbId));
    co_return tmdb::parseSeriesExternalIds(co_await fetch(url));
}

QCoro::Task<std::pair<int, MediaKind>> TmdbClient::findByImdb(
    QString imdbId, MediaKind preferredKind)
{
    requireToken();
    const auto url = buildUrl(QStringLiteral("/find/%1").arg(imdbId),
        { { QStringLiteral("external_source"),
            QStringLiteral("imdb_id") } });
    co_return tmdb::parseFindResult(co_await fetch(url), preferredKind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::recommendations(MediaKind kind, int tmdbId)
{
    requireToken();
    const auto url = buildUrl(QStringLiteral("/%1/%2/recommendations")
            .arg(kindMovieOrTv(kind), QString::number(tmdbId)));
    co_return tmdb::parseList(co_await fetch(url), kind);
}

QCoro::Task<QList<DiscoverItem>> TmdbClient::similar(MediaKind kind, int tmdbId)
{
    requireToken();
    const auto url = buildUrl(QStringLiteral("/%1/%2/similar")
            .arg(kindMovieOrTv(kind), QString::number(tmdbId)));
    co_return tmdb::parseList(co_await fetch(url), kind);
}

QCoro::Task<DiscoverPageResult> TmdbClient::discover(DiscoverQuery q)
{
    requireToken();
    const auto url = buildUrl(tmdb::discoverPath(q.kind),
        tmdb::discoverQueryToQuery(q));
    co_return tmdb::parsePagedList(co_await fetch(url), q.kind);
}

QCoro::Task<QList<TmdbGenre>> TmdbClient::genreList(MediaKind kind)
{
    auto& cache = kind == MediaKind::Movie ? m_genresMovie : m_genresSeries;
    if (!cache.isEmpty()) {
        co_return cache;
    }
    requireToken();
    const auto url = buildUrl(
        QStringLiteral("/genre/%1/list").arg(kindMovieOrTv(kind)));
    cache = tmdb::parseGenreList(co_await fetch(url));
    co_return cache;
}

QCoro::Task<void> TmdbClient::testAuth()
{
    requireToken();
    (void)co_await fetch(buildUrl(QStringLiteral("/authentication")));
}

} // namespace kinema::api
