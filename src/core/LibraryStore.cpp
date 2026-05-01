// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/LibraryStore.h"

#include "core/Database.h"
#include "core/SqlUtil.h"
#include "kinema_debug.h"

#include <QDate>
#include <QDateTime>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QTimer>
#include <QVariant>

namespace kinema::core {

namespace {

using sql::isoUtc;
using sql::nullSafe;
using sql::parseIsoUtc;

QString mediaKindToDb(api::MediaKind k)
{
    return k == api::MediaKind::Movie
        ? QStringLiteral("movie")
        : QStringLiteral("series");
}

api::MediaKind mediaKindFromDb(const QString& s)
{
    return s == QStringLiteral("series")
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
}

QString dateToDb(const std::optional<QDate>& date)
{
    return date && date->isValid()
        ? date->toString(Qt::ISODate)
        : QString();
}

std::optional<QDate> dateFromDb(const QString& s)
{
    if (s.isEmpty()) {
        return std::nullopt;
    }
    const auto d = QDate::fromString(s, Qt::ISODate);
    if (!d.isValid()) {
        return std::nullopt;
    }
    return d;
}

constexpr const char* kTitleColumns =
    "kind, imdb_id, tmdb_id, title, year, poster_url, backdrop_url, "
    "overview, release_date, active, added_at, updated_at";

constexpr const char* kEpisodeColumns =
    "series_imdb_id, season, episode, title, overview, thumbnail_url, "
    "release_date, updated_at";

api::LibraryTitle hydrateTitle(const QSqlQuery& q)
{
    api::LibraryTitle t;
    t.kind = mediaKindFromDb(q.value(0).toString());
    t.imdbId = q.value(1).toString();
    t.tmdbId = q.value(2).toInt();
    t.title = q.value(3).toString();
    if (!q.value(4).isNull()) {
        t.year = q.value(4).toInt();
    }
    const auto poster = q.value(5).toString();
    if (!poster.isEmpty()) {
        t.poster = QUrl(poster);
    }
    const auto backdrop = q.value(6).toString();
    if (!backdrop.isEmpty()) {
        t.backdrop = QUrl(backdrop);
    }
    t.overview = q.value(7).toString();
    t.releaseDate = dateFromDb(q.value(8).toString());
    t.active = q.value(9).toInt() != 0;
    t.addedAt = parseIsoUtc(q.value(10).toString());
    t.updatedAt = parseIsoUtc(q.value(11).toString());
    return t;
}

api::LibraryEpisode hydrateEpisode(const QSqlQuery& q)
{
    api::LibraryEpisode e;
    e.seriesImdbId = q.value(0).toString();
    e.season = q.value(1).toInt();
    e.episode = q.value(2).toInt();
    e.title = q.value(3).toString();
    e.overview = q.value(4).toString();
    const auto thumb = q.value(5).toString();
    if (!thumb.isEmpty()) {
        e.thumbnail = QUrl(thumb);
    }
    e.releaseDate = dateFromDb(q.value(6).toString());
    e.updatedAt = parseIsoUtc(q.value(7).toString());
    return e;
}

bool validWatchState(api::LibraryWatchOverride state)
{
    return state == api::LibraryWatchOverride::Watched
        || state == api::LibraryWatchOverride::Unwatched;
}

api::LibraryWatchOverride watchStateFromDb(int state)
{
    switch (state) {
    case 1: return api::LibraryWatchOverride::Watched;
    case 2: return api::LibraryWatchOverride::Unwatched;
    default: return api::LibraryWatchOverride::None;
    }
}

} // namespace

LibraryStore::LibraryStore(Database& db, QObject* parent)
    : QObject(parent)
    , m_db(db)
{
}

LibraryStore::~LibraryStore() = default;

std::optional<api::LibraryTitle> LibraryStore::find(
    api::MediaKind kind, const QString& imdbId) const
{
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return std::nullopt;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT ") + QString::fromLatin1(kTitleColumns)
        + QStringLiteral(" FROM library_titles WHERE kind = ? AND imdb_id = ?"));
    q.addBindValue(mediaKindToDb(kind));
    q.addBindValue(imdbId);
    if (!q.exec() || !q.next()) {
        return std::nullopt;
    }
    return hydrateTitle(q);
}

bool LibraryStore::isActive(api::MediaKind kind, const QString& imdbId) const
{
    const auto t = find(kind, imdbId);
    return t && t->active;
}

QList<api::LibraryTitle> LibraryStore::activeTitles() const
{
    return allTitles(false);
}

QList<api::LibraryTitle> LibraryStore::allTitles(bool includeInactive) const
{
    QList<api::LibraryTitle> out;
    if (!m_db.isOpen()) {
        return out;
    }
    auto q = m_db.query();
    const QString where = includeInactive ? QString() : QStringLiteral(" WHERE active = 1");
    if (!q.exec(QStringLiteral("SELECT ") + QString::fromLatin1(kTitleColumns)
            + QStringLiteral(" FROM library_titles") + where
            + QStringLiteral(" ORDER BY updated_at DESC"))) {
        qCWarning(KINEMA) << "LibraryStore: allTitles failed:"
                          << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.append(hydrateTitle(q));
    }
    return out;
}

QList<api::LibraryEpisode> LibraryStore::episodesForSeries(
    const QString& imdbId) const
{
    QList<api::LibraryEpisode> out;
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return out;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT ") + QString::fromLatin1(kEpisodeColumns)
        + QStringLiteral(" FROM library_episodes WHERE series_imdb_id = ? "
                         "ORDER BY season ASC, episode ASC"));
    q.addBindValue(imdbId);
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: episodesForSeries failed:"
                          << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.append(hydrateEpisode(q));
    }
    return out;
}

std::optional<api::LibraryEpisode> LibraryStore::episode(
    const QString& imdbId, int season, int episodeNumber) const
{
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return std::nullopt;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT ") + QString::fromLatin1(kEpisodeColumns)
        + QStringLiteral(" FROM library_episodes WHERE series_imdb_id = ? "
                         "AND season = ? AND episode = ?"));
    q.addBindValue(imdbId);
    q.addBindValue(season);
    q.addBindValue(episodeNumber);
    if (!q.exec() || !q.next()) {
        return std::nullopt;
    }
    return hydrateEpisode(q);
}

void LibraryStore::upsertTitle(const api::LibraryTitle& title)
{
    if (!m_db.isOpen() || title.imdbId.isEmpty() || title.title.isEmpty()) {
        return;
    }

    const auto now = QDateTime::currentDateTimeUtc();
    auto t = title;
    if (!t.addedAt.isValid()) {
        if (const auto existing = find(t.kind, t.imdbId)) {
            t.addedAt = existing->addedAt;
        } else {
            t.addedAt = now;
        }
    }
    if (!t.updatedAt.isValid()) {
        t.updatedAt = now;
    }

    auto q = m_db.query();
    q.prepare(QStringLiteral(R"(
        INSERT INTO library_titles (
            kind, imdb_id, tmdb_id, title, year, poster_url, backdrop_url,
            overview, release_date, active, added_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(kind, imdb_id) DO UPDATE SET
            tmdb_id = excluded.tmdb_id,
            title = excluded.title,
            year = excluded.year,
            poster_url = excluded.poster_url,
            backdrop_url = excluded.backdrop_url,
            overview = excluded.overview,
            release_date = excluded.release_date,
            active = excluded.active,
            updated_at = excluded.updated_at
    )"));
    q.addBindValue(mediaKindToDb(t.kind));
    q.addBindValue(t.imdbId);
    q.addBindValue(t.tmdbId);
    q.addBindValue(nullSafe(t.title));
    q.addBindValue(t.year ? QVariant(*t.year) : QVariant());
    q.addBindValue(nullSafe(t.poster.isValid() ? t.poster.toString() : QString()));
    q.addBindValue(nullSafe(t.backdrop.isValid() ? t.backdrop.toString() : QString()));
    q.addBindValue(nullSafe(t.overview));
    q.addBindValue(nullSafe(dateToDb(t.releaseDate)));
    q.addBindValue(t.active ? 1 : 0);
    q.addBindValue(isoUtc(t.addedAt));
    q.addBindValue(isoUtc(t.updatedAt));
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: upsertTitle failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void LibraryStore::upsertEpisodes(const QString& seriesImdbId,
    const QList<api::LibraryEpisode>& episodes)
{
    if (!m_db.isOpen() || seriesImdbId.isEmpty()) {
        return;
    }
    auto db = QSqlDatabase::database(m_db.connectionName());
    if (!db.transaction()) {
        qCWarning(KINEMA) << "LibraryStore: episode transaction failed:"
                          << db.lastError().text();
        return;
    }

    auto q = m_db.query();
    q.prepare(QStringLiteral(R"(
        INSERT INTO library_episodes (
            series_imdb_id, season, episode, title, overview, thumbnail_url,
            release_date, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(series_imdb_id, season, episode) DO UPDATE SET
            title = excluded.title,
            overview = excluded.overview,
            thumbnail_url = excluded.thumbnail_url,
            release_date = excluded.release_date,
            updated_at = excluded.updated_at
    )"));

    const auto now = QDateTime::currentDateTimeUtc();
    for (const auto& in : episodes) {
        if (in.season < 0 || in.episode <= 0) {
            continue;
        }
        const auto updated = in.updatedAt.isValid() ? in.updatedAt : now;
        q.addBindValue(seriesImdbId);
        q.addBindValue(in.season);
        q.addBindValue(in.episode);
        q.addBindValue(nullSafe(in.title));
        q.addBindValue(nullSafe(in.overview));
        q.addBindValue(nullSafe(in.thumbnail.isValid() ? in.thumbnail.toString() : QString()));
        q.addBindValue(nullSafe(dateToDb(in.releaseDate)));
        q.addBindValue(isoUtc(updated));
        if (!q.exec()) {
            qCWarning(KINEMA) << "LibraryStore: upsertEpisodes failed:"
                              << q.lastError().text();
            db.rollback();
            return;
        }
    }

    if (!db.commit()) {
        qCWarning(KINEMA) << "LibraryStore: episode commit failed:"
                          << db.lastError().text();
        db.rollback();
        return;
    }
    scheduleChanged();
}

void LibraryStore::setActive(api::MediaKind kind, const QString& imdbId,
    bool active)
{
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "UPDATE library_titles SET active = ?, updated_at = ? "
        "WHERE kind = ? AND imdb_id = ?"));
    q.addBindValue(active ? 1 : 0);
    q.addBindValue(isoUtc(QDateTime::currentDateTimeUtc()));
    q.addBindValue(mediaKindToDb(kind));
    q.addBindValue(imdbId);
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: setActive failed:"
                          << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        scheduleChanged();
    }
}

void LibraryStore::hardDelete(api::MediaKind kind, const QString& imdbId)
{
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return;
    }
    auto db = QSqlDatabase::database(m_db.connectionName());
    if (!db.transaction()) {
        qCWarning(KINEMA) << "LibraryStore: hardDelete transaction failed:"
                          << db.lastError().text();
        return;
    }

    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "DELETE FROM library_titles WHERE kind = ? AND imdb_id = ?"));
    q.addBindValue(mediaKindToDb(kind));
    q.addBindValue(imdbId);
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: hardDelete title failed:"
                          << q.lastError().text();
        db.rollback();
        return;
    }

    if (kind == api::MediaKind::Series) {
        q.prepare(QStringLiteral(
            "DELETE FROM library_episodes WHERE series_imdb_id = ?"));
        q.addBindValue(imdbId);
        if (!q.exec()) {
            qCWarning(KINEMA) << "LibraryStore: hardDelete episodes failed:"
                              << q.lastError().text();
            db.rollback();
            return;
        }
        q.prepare(QStringLiteral(
            "DELETE FROM library_watch_overrides WHERE imdb_id = ?"));
        q.addBindValue(imdbId);
    } else {
        api::PlaybackKey key;
        key.kind = api::MediaKind::Movie;
        key.imdbId = imdbId;
        q.prepare(QStringLiteral(
            "DELETE FROM library_watch_overrides WHERE key = ?"));
        q.addBindValue(key.storageKey());
    }
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: hardDelete overrides failed:"
                          << q.lastError().text();
        db.rollback();
        return;
    }

    if (!db.commit()) {
        qCWarning(KINEMA) << "LibraryStore: hardDelete commit failed:"
                          << db.lastError().text();
        db.rollback();
        return;
    }
    scheduleChanged();
}

api::LibraryWatchOverride LibraryStore::watchOverride(
    const api::PlaybackKey& key) const
{
    if (!m_db.isOpen() || !key.isValid()) {
        return api::LibraryWatchOverride::None;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "SELECT state FROM library_watch_overrides WHERE key = ?"));
    q.addBindValue(key.storageKey());
    if (!q.exec() || !q.next()) {
        return api::LibraryWatchOverride::None;
    }
    return watchStateFromDb(q.value(0).toInt());
}

QHash<QString, api::LibraryWatchOverride> LibraryStore::watchOverridesForImdb(
    const QString& imdbId) const
{
    QHash<QString, api::LibraryWatchOverride> out;
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return out;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "SELECT key, state FROM library_watch_overrides WHERE imdb_id = ?"));
    q.addBindValue(imdbId);
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: watchOverridesForImdb failed:"
                          << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.insert(q.value(0).toString(), watchStateFromDb(q.value(1).toInt()));
    }
    return out;
}

void LibraryStore::setWatchOverride(const api::PlaybackKey& key,
    api::LibraryWatchOverride state)
{
    if (!m_db.isOpen() || !key.isValid()) {
        return;
    }
    if (!validWatchState(state)) {
        clearWatchOverride(key);
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(R"(
        INSERT INTO library_watch_overrides (
            key, kind, imdb_id, season, episode, state, changed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(key) DO UPDATE SET
            kind = excluded.kind,
            imdb_id = excluded.imdb_id,
            season = excluded.season,
            episode = excluded.episode,
            state = excluded.state,
            changed_at = excluded.changed_at
    )"));
    q.addBindValue(key.storageKey());
    q.addBindValue(mediaKindToDb(key.kind));
    q.addBindValue(key.imdbId);
    q.addBindValue(key.season ? QVariant(*key.season) : QVariant());
    q.addBindValue(key.episode ? QVariant(*key.episode) : QVariant());
    q.addBindValue(static_cast<int>(state));
    q.addBindValue(isoUtc(QDateTime::currentDateTimeUtc()));
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: setWatchOverride failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void LibraryStore::clearWatchOverride(const api::PlaybackKey& key)
{
    if (!m_db.isOpen() || !key.isValid()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "DELETE FROM library_watch_overrides WHERE key = ?"));
    q.addBindValue(key.storageKey());
    if (!q.exec()) {
        qCWarning(KINEMA) << "LibraryStore: clearWatchOverride failed:"
                          << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        scheduleChanged();
    }
}

void LibraryStore::scheduleChanged()
{
    if (m_changePending) {
        return;
    }
    m_changePending = true;
    QTimer::singleShot(0, this, [this] {
        m_changePending = false;
        Q_EMIT changed();
    });
}

} // namespace kinema::core
