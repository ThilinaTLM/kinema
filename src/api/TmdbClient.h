// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"

#include <QHash>
#include <QNetworkRequest>
#include <QObject>
#include <QString>
#include <QUrl>
#include <QUrlQuery>

#include <QCoro/QCoroTask>

#include <initializer_list>
#include <utility>

namespace kinema::core {
class HttpClient;
}

namespace kinema::api {

/**
 * Client for The Movie Database (TMDB) v3 API, authenticated with a v4
 * bearer Read Access Token. Powers the Discover surface (home rows +
 * "More like this" strips). Search and per-title metadata still go
 * through CinemetaClient.
 *
 * Endpoint reference (all prefixed with `/3` off api.themoviedb.org):
 *
 *   /trending/{movie|tv}/{day|week}
 *   /movie/popular, /tv/popular
 *   /movie/top_rated, /tv/top_rated
 *   /movie/now_playing
 *   /tv/on_the_air
 *   /movie/{id}?append_to_response=external_ids
 *   /tv/{id}/external_ids
 *   /find/{imdb_id}?external_source=imdb_id
 *   /movie/{id}/recommendations, /tv/{id}/recommendations
 *   /movie/{id}/similar, /tv/{id}/similar
 *   /authentication     (validate bearer token)
 *
 * `language` and `region` query params come from QLocale::system() by
 * default. No hard rate limit today; all requests are plain GETs.
 */
class TmdbClient : public QObject
{
    Q_OBJECT
public:
    explicit TmdbClient(core::HttpClient* http, QObject* parent = nullptr);

    /// Override the TMDB base URL (default: https://api.themoviedb.org/3).
    void setBaseUrl(QUrl url);
    const QUrl& baseUrl() const noexcept { return m_baseUrl; }

    /// v4 Read Access Token — sent as `Authorization: Bearer <token>`.
    /// In-memory only; the caller reads from / writes to the keyring.
    void setToken(QString token);
    bool hasToken() const noexcept { return !m_token.isEmpty(); }

    /// Override the language / region. Defaults are derived from the
    /// system locale (e.g. "en-US", "US"). Called by MainWindow at
    /// construction; callers may override for tests.
    void setLanguage(QString language);
    void setRegion(QString region);

    // ---- M1 catalog rows ---------------------------------------------------

    QCoro::Task<QList<DiscoverItem>> trending(MediaKind kind, bool weekly = true);
    QCoro::Task<QList<DiscoverItem>> popular(MediaKind kind);
    QCoro::Task<QList<DiscoverItem>> topRated(MediaKind kind);
    QCoro::Task<QList<DiscoverItem>> nowPlayingMovies();
    QCoro::Task<QList<DiscoverItem>> onTheAirSeries();

    // ---- Browse (filterable /discover) ------------------------------------

    /// Hit /discover/{movie|tv} with the caller's filters. Returns the
    /// requested page plus paging metadata. See TmdbDiscoverUrl.h for
    /// the exact query-string mapping.
    QCoro::Task<DiscoverPageResult> discover(DiscoverQuery q);

    /// Fetch the genre list for a given kind. Results are memoised per
    /// kind; the second call returns immediately. Invalidated by
    /// setLanguage() since names are localised server-side.
    QCoro::Task<QList<TmdbGenre>> genreList(MediaKind kind);

    // ---- Click-through resolution -----------------------------------------

    /// Fetch /movie/{id}?append_to_response=external_ids and return the
    /// IMDB id (or an empty string if the title has none on TMDB).
    QCoro::Task<QString> imdbIdForTmdbMovie(int tmdbId);

    /// Fetch /tv/{id}/external_ids and return the IMDB id.
    QCoro::Task<QString> imdbIdForTmdbSeries(int tmdbId);

    /// Resolve an IMDB id to a (tmdbId, kind) pair via /find. `preferredKind`
    /// disambiguates when both movie_results and tv_results are populated
    /// (happens rarely but does). Returns (0, preferredKind) on miss.
    QCoro::Task<std::pair<int, MediaKind>> findByImdb(
        QString imdbId, MediaKind preferredKind);

    // ---- Per-item recommendations ------------------------------------------

    QCoro::Task<QList<DiscoverItem>> recommendations(MediaKind kind, int tmdbId);
    QCoro::Task<QList<DiscoverItem>> similar(MediaKind kind, int tmdbId);

    // ---- Auth ping ---------------------------------------------------------

    /// Hit /authentication — a 200 means the bearer is valid.
    /// Throws HttpError on any non-2xx (401 for an invalid token).
    QCoro::Task<void> testAuth();

private:
    QUrl buildUrl(const QString& path,
        std::initializer_list<std::pair<QString, QString>> extra = {}) const;
    QNetworkRequest authed(const QUrl& url) const;

    /// Overload that accepts arbitrary extra query items (used by
    /// /discover where params like `sort_by`, `with_genres`, etc. are
    /// pre-built into a QUrlQuery).
    QUrl buildUrl(const QString& path, const QUrlQuery& extra) const;

    core::HttpClient* m_http;
    QUrl m_baseUrl;
    QString m_token;
    QString m_language;
    QString m_region;

    // Per-kind genre-list cache. Empty = not yet fetched. Populated on
    // first genreList(kind) call; cleared by setLanguage().
    QList<TmdbGenre> m_genresMovie;
    QList<TmdbGenre> m_genresSeries;
};

} // namespace kinema::api
