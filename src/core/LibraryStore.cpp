// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/LibraryStore.h"

#include "core/Database.h"
#include "core/SqlUtil.h"
#include "kinema_log_app.h"

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
    "overview, release_date, added_at, updated_at, "
    "genres, imdb_rating, runtime_minutes, cast_list";

/// Field separator for list-valued columns (`genres`, `cast_list`).
/// We use ASCII Unit-Separator (\u001f) so values containing commas
/// or semicolons (“Actor, Jr.”, “Sci-Fi & Fantasy”) round-trip
/// without ambiguous splitting.
constexpr QChar kListSep = QChar(0x001F);

QString joinList(const QStringList& xs)
{
    return xs.join(kListSep);
}

QStringList splitList(const QString& s)
{
    if (s.isEmpty()) {
        return {};
    }
    return s.split(kListSep, Qt::SkipEmptyParts);
}

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
    t.addedAt = parseIsoUtc(q.value(9).toString());
    t.updatedAt = parseIsoUtc(q.value(10).toString());
    t.genres = splitList(q.value(11).toString());
    if (!q.value(12).isNull()) {
        t.imdbRating = q.value(12).toDouble();
    }
    if (!q.value(13).isNull()) {
        t.runtimeMinutes = q.value(13).toInt();
    }
    t.cast = splitList(q.value(14).toString());
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

bool LibraryStore::contains(api::MediaKind kind,
    const QString& imdbId) const
{
    return find(kind, imdbId).has_value();
}

QList<api::LibraryTitle> LibraryStore::titles() const
{
    QList<api::LibraryTitle> out;
    if (!m_db.isOpen()) {
        return out;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("SELECT ") + QString::fromLatin1(kTitleColumns)
            + QStringLiteral(" FROM library_titles ORDER BY updated_at DESC"))) {
        qCWarning(KINEMA_APP) << "LibraryStore: titles failed:"
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
        qCWarning(KINEMA_APP) << "LibraryStore: episodesForSeries failed:"
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
            overview, release_date, added_at, updated_at,
            genres, imdb_rating, runtime_minutes, cast_list
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(kind, imdb_id) DO UPDATE SET
            tmdb_id = excluded.tmdb_id,
            title = excluded.title,
            year = excluded.year,
            poster_url = excluded.poster_url,
            backdrop_url = excluded.backdrop_url,
            overview = excluded.overview,
            release_date = excluded.release_date,
            updated_at = excluded.updated_at,
            genres = excluded.genres,
            imdb_rating = excluded.imdb_rating,
            runtime_minutes = excluded.runtime_minutes,
            cast_list = excluded.cast_list
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
    q.addBindValue(isoUtc(t.addedAt));
    q.addBindValue(isoUtc(t.updatedAt));
    q.addBindValue(nullSafe(joinList(t.genres)));
    q.addBindValue(t.imdbRating ? QVariant(*t.imdbRating) : QVariant());
    q.addBindValue(t.runtimeMinutes ? QVariant(*t.runtimeMinutes) : QVariant());
    q.addBindValue(nullSafe(joinList(t.cast)));
    if (!q.exec()) {
        qCWarning(KINEMA_APP) << "LibraryStore: upsertTitle failed:"
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
        qCWarning(KINEMA_APP) << "LibraryStore: episode transaction failed:"
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
            qCWarning(KINEMA_APP) << "LibraryStore: upsertEpisodes failed:"
                              << q.lastError().text();
            db.rollback();
            return;
        }
    }

    if (!db.commit()) {
        qCWarning(KINEMA_APP) << "LibraryStore: episode commit failed:"
                          << db.lastError().text();
        db.rollback();
        return;
    }
    scheduleChanged();
}

void LibraryStore::remove(api::MediaKind kind, const QString& imdbId)
{
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return;
    }
    auto db = QSqlDatabase::database(m_db.connectionName());
    if (!db.transaction()) {
        qCWarning(KINEMA_APP) << "LibraryStore: remove transaction failed:"
                          << db.lastError().text();
        return;
    }

    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "DELETE FROM library_titles WHERE kind = ? AND imdb_id = ?"));
    q.addBindValue(mediaKindToDb(kind));
    q.addBindValue(imdbId);
    if (!q.exec()) {
        qCWarning(KINEMA_APP) << "LibraryStore: remove title failed:"
                          << q.lastError().text();
        db.rollback();
        return;
    }
    const auto deletedTitles = q.numRowsAffected();

    if (kind == api::MediaKind::Series) {
        q.prepare(QStringLiteral(
            "DELETE FROM library_episodes WHERE series_imdb_id = ?"));
        q.addBindValue(imdbId);
        if (!q.exec()) {
            qCWarning(KINEMA_APP) << "LibraryStore: remove episodes failed:"
                              << q.lastError().text();
            db.rollback();
            return;
        }
    }

    if (!db.commit()) {
        qCWarning(KINEMA_APP) << "LibraryStore: remove commit failed:"
                          << db.lastError().text();
        db.rollback();
        return;
    }
    if (deletedTitles > 0) {
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
