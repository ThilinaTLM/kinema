// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/HistoryStore.h"

#include "core/Database.h"
#include "kinema_debug.h"

#include <QDateTime>
#include <QSqlError>
#include <QSqlQuery>
#include <QTimeZone>
#include <QTimer>
#include <QVariant>

namespace kinema::core {

namespace {

constexpr const char* kIsoFmt = "yyyy-MM-ddTHH:mm:ssZ";

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

QString isoUtc(const QDateTime& dt)
{
    return dt.toUTC().toString(QString::fromLatin1(kIsoFmt));
}

/// Qt's QSQLITE driver binds a null QString as SQL NULL, which
/// clashes with our `NOT NULL DEFAULT ''` columns. Normalise first.
QString nullSafe(const QString& s)
{
    return s.isNull() ? QString::fromLatin1("") : s;
}

QDateTime parseIsoUtc(const QString& s)
{
    auto dt = QDateTime::fromString(s, Qt::ISODate);
    if (!dt.isValid()) {
        dt = QDateTime::fromString(s, QString::fromLatin1(kIsoFmt));
    }
    if (dt.isValid()) {
        dt.setTimeZone(QTimeZone::utc());
    }
    return dt;
}

/// Column layout consumed by `hydrate()`. Any SELECT that feeds
/// hydrate() MUST select these columns in this exact order.
constexpr const char* kSelectColumns =
    "key, kind, imdb_id, season, episode, "
    "title, series_title, episode_title, poster_url, "
    "position_sec, duration_sec, finished, last_watched_at, "
    "stream_info_hash, stream_release_name, stream_resolution, "
    "stream_quality_label, stream_size_bytes, stream_provider";

api::HistoryEntry hydrate(const QSqlQuery& q)
{
    api::HistoryEntry e;
    e.key.kind = mediaKindFromDb(q.value(1).toString());
    e.key.imdbId = q.value(2).toString();
    if (!q.value(3).isNull()) {
        e.key.season = q.value(3).toInt();
    }
    if (!q.value(4).isNull()) {
        e.key.episode = q.value(4).toInt();
    }
    e.title = q.value(5).toString();
    e.seriesTitle = q.value(6).toString();
    e.episodeTitle = q.value(7).toString();
    const auto posterStr = q.value(8).toString();
    if (!posterStr.isEmpty()) {
        e.poster = QUrl(posterStr);
    }
    e.positionSec = q.value(9).toDouble();
    e.durationSec = q.value(10).toDouble();
    e.finished = q.value(11).toInt() != 0;
    e.lastWatchedAt = parseIsoUtc(q.value(12).toString());
    e.lastStream.infoHash = q.value(13).toString();
    e.lastStream.releaseName = q.value(14).toString();
    e.lastStream.resolution = q.value(15).toString();
    e.lastStream.qualityLabel = q.value(16).toString();
    if (!q.value(17).isNull()) {
        e.lastStream.sizeBytes = q.value(17).toLongLong();
    }
    e.lastStream.provider = q.value(18).toString();
    return e;
}

} // namespace

HistoryStore::HistoryStore(Database& db, QObject* parent)
    : QObject(parent)
    , m_db(db)
{
}

HistoryStore::~HistoryStore() = default;

void HistoryStore::setRetentionDays(int days)
{
    if (days > 0) {
        m_retentionDays = days;
    }
}

void HistoryStore::setFinishedThreshold(double fraction)
{
    m_finishedThreshold = qBound(0.5, fraction, 1.0);
}

void HistoryStore::runRetentionPass()
{
    if (!m_db.isOpen()) {
        return;
    }
    const auto cutoff = QDateTime::currentDateTimeUtc()
                            .addDays(-m_retentionDays);
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "DELETE FROM history "
        "WHERE finished = 1 AND last_watched_at < ?"));
    q.addBindValue(isoUtc(cutoff));
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "HistoryStore: retention pass failed:"
            << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        qCInfo(KINEMA) << "HistoryStore: retention removed"
                       << q.numRowsAffected() << "finished rows";
        scheduleChanged();
    }
}

std::optional<api::HistoryEntry> HistoryStore::find(
    const api::PlaybackKey& key) const
{
    if (!m_db.isOpen() || !key.isValid()) {
        return std::nullopt;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT ") + QString::fromLatin1(kSelectColumns)
        + QStringLiteral(" FROM history WHERE key = ?"));
    q.addBindValue(key.storageKey());
    if (!q.exec() || !q.next()) {
        return std::nullopt;
    }
    return hydrate(q);
}

std::optional<api::HistoryEntry> HistoryStore::findLatestForMedia(
    api::MediaKind kind, const QString& imdbId) const
{
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return std::nullopt;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT ") + QString::fromLatin1(kSelectColumns)
        + QStringLiteral(
            " FROM history WHERE kind = ? AND imdb_id = ? "
            "ORDER BY last_watched_at DESC LIMIT 1"));
    q.addBindValue(mediaKindToDb(kind));
    q.addBindValue(imdbId);
    if (!q.exec() || !q.next()) {
        return std::nullopt;
    }
    return hydrate(q);
}

QList<api::HistoryEntry> HistoryStore::continueWatching(int limit) const
{
    QList<api::HistoryEntry> out;
    if (!m_db.isOpen() || limit <= 0) {
        return out;
    }

    // We want, per imdb_id, the most recently watched non-finished
    // row. A window function would be nicest but stays compatible
    // across Qt's bundled SQLite versions; using a correlated subquery
    // keeps the statement portable and still hits the
    // (imdb_id, last_watched_at DESC) index.
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT ") + QString::fromLatin1(kSelectColumns)
        + QStringLiteral(R"( FROM history h
            WHERE finished = 0
              AND last_watched_at = (
                SELECT MAX(last_watched_at) FROM history
                WHERE imdb_id = h.imdb_id AND finished = 0
              )
            ORDER BY last_watched_at DESC
            LIMIT ?)"));
    q.addBindValue(limit);
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "HistoryStore: continueWatching failed:"
            << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.append(hydrate(q));
    }
    return out;
}

void HistoryStore::record(const api::HistoryEntry& entry)
{
    if (!m_db.isOpen() || !entry.key.isValid()) {
        return;
    }

    api::HistoryEntry e = entry;
    if (!e.lastWatchedAt.isValid()) {
        e.lastWatchedAt = QDateTime::currentDateTimeUtc();
    } else {
        e.lastWatchedAt = e.lastWatchedAt.toUTC();
    }

    // Auto-flip `finished` when we cross the threshold and duration is
    // known. Never un-finish here \u2014 callers that reset progress do so
    // deliberately.
    if (!e.finished && e.durationSec > 0.0
        && e.positionSec / e.durationSec >= m_finishedThreshold) {
        e.finished = true;
    }

    const auto keyStr = e.key.storageKey();
    auto q = m_db.query();

    // Upsert. The progress-only update case uses COALESCE over
    // stream_* columns so a blank HistoryStreamRef preserves the
    // previously-persisted release reference.
    q.prepare(QStringLiteral(R"(
        INSERT INTO history (
            key, kind, imdb_id, season, episode,
            title, series_title, episode_title, poster_url,
            position_sec, duration_sec, finished, last_watched_at,
            stream_info_hash, stream_release_name, stream_resolution,
            stream_quality_label, stream_size_bytes, stream_provider
        ) VALUES (
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?
        )
        ON CONFLICT(key) DO UPDATE SET
            kind = excluded.kind,
            imdb_id = excluded.imdb_id,
            season = excluded.season,
            episode = excluded.episode,
            title = CASE WHEN excluded.title != '' THEN excluded.title
                         ELSE history.title END,
            series_title = CASE WHEN excluded.series_title != ''
                                THEN excluded.series_title
                                ELSE history.series_title END,
            episode_title = CASE WHEN excluded.episode_title != ''
                                 THEN excluded.episode_title
                                 ELSE history.episode_title END,
            poster_url = CASE WHEN excluded.poster_url != ''
                              THEN excluded.poster_url
                              ELSE history.poster_url END,
            position_sec = excluded.position_sec,
            duration_sec = CASE WHEN excluded.duration_sec > 0
                                THEN excluded.duration_sec
                                ELSE history.duration_sec END,
            finished = excluded.finished,
            last_watched_at = excluded.last_watched_at,
            stream_info_hash = CASE WHEN excluded.stream_info_hash != ''
                                    THEN excluded.stream_info_hash
                                    ELSE history.stream_info_hash END,
            stream_release_name = CASE WHEN excluded.stream_info_hash != ''
                                       THEN excluded.stream_release_name
                                       ELSE history.stream_release_name END,
            stream_resolution = CASE WHEN excluded.stream_info_hash != ''
                                     THEN excluded.stream_resolution
                                     ELSE history.stream_resolution END,
            stream_quality_label = CASE WHEN excluded.stream_info_hash != ''
                                        THEN excluded.stream_quality_label
                                        ELSE history.stream_quality_label END,
            stream_size_bytes = CASE WHEN excluded.stream_info_hash != ''
                                     THEN excluded.stream_size_bytes
                                     ELSE history.stream_size_bytes END,
            stream_provider = CASE WHEN excluded.stream_info_hash != ''
                                   THEN excluded.stream_provider
                                   ELSE history.stream_provider END
    )"));

    q.addBindValue(keyStr);
    q.addBindValue(mediaKindToDb(e.key.kind));
    q.addBindValue(e.key.imdbId);
    q.addBindValue(e.key.season ? QVariant(*e.key.season) : QVariant());
    q.addBindValue(e.key.episode ? QVariant(*e.key.episode) : QVariant());
    q.addBindValue(nullSafe(e.title));
    q.addBindValue(nullSafe(e.seriesTitle));
    q.addBindValue(nullSafe(e.episodeTitle));
    q.addBindValue(nullSafe(e.poster.isValid() ? e.poster.toString() : QString()));
    q.addBindValue(e.positionSec);
    q.addBindValue(e.durationSec);
    q.addBindValue(e.finished ? 1 : 0);
    q.addBindValue(isoUtc(e.lastWatchedAt));
    q.addBindValue(nullSafe(e.lastStream.infoHash));
    q.addBindValue(nullSafe(e.lastStream.releaseName));
    q.addBindValue(nullSafe(e.lastStream.resolution));
    q.addBindValue(nullSafe(e.lastStream.qualityLabel));
    q.addBindValue(e.lastStream.sizeBytes
            ? QVariant(*e.lastStream.sizeBytes)
            : QVariant());
    q.addBindValue(nullSafe(e.lastStream.provider));

    if (!q.exec()) {
        qCWarning(KINEMA)
            << "HistoryStore: upsert failed for" << keyStr
            << "\u2014" << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void HistoryStore::remove(const api::PlaybackKey& key)
{
    if (!m_db.isOpen() || !key.isValid()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("DELETE FROM history WHERE key = ?"));
    q.addBindValue(key.storageKey());
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "HistoryStore: remove failed:" << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        scheduleChanged();
    }
}

void HistoryStore::clear()
{
    if (!m_db.isOpen()) {
        return;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("DELETE FROM history"))) {
        qCWarning(KINEMA)
            << "HistoryStore: clear failed:" << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void HistoryStore::scheduleChanged()
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
