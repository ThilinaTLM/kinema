// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/PlayQueueStore.h"

#include "core/Database.h"
#include "core/SqlUtil.h"
#include "kinema_debug.h"

#include <QDateTime>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QVariant>

namespace kinema::core {

namespace {

using sql::isoUtc;
using sql::nullSafe;
using sql::parseIsoUtc;

constexpr const char* kSelectColumns =
    "id, ord, imdb_id, media_kind, season, episode, "
    "title, series_title, episode_title, poster, "
    "info_hash, release_name, resolution, quality_label, "
    "size_bytes, provider, added_at";

api::QueueItem hydrate(const QSqlQuery& q)
{
    api::QueueItem it;
    it.id = q.value(0).toLongLong();
    it.order = q.value(1).toInt();
    it.key.imdbId = q.value(2).toString();
    it.key.kind = q.value(3).toInt() == 1
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
    if (!q.value(4).isNull()) {
        it.key.season = q.value(4).toInt();
    }
    if (!q.value(5).isNull()) {
        it.key.episode = q.value(5).toInt();
    }
    it.title = q.value(6).toString();
    it.seriesTitle = q.value(7).toString();
    it.episodeTitle = q.value(8).toString();
    const auto posterStr = q.value(9).toString();
    if (!posterStr.isEmpty()) {
        it.poster = QUrl(posterStr);
    }
    it.streamRef.infoHash = q.value(10).toString();
    it.streamRef.releaseName = q.value(11).toString();
    it.streamRef.resolution = q.value(12).toString();
    it.streamRef.qualityLabel = q.value(13).toString();
    if (!q.value(14).isNull()) {
        it.streamRef.sizeBytes = q.value(14).toLongLong();
    }
    it.streamRef.provider = q.value(15).toString();
    it.addedAt = parseIsoUtc(q.value(16).toString());
    return it;
}

void bindInsertValues(QSqlQuery& q, const api::QueueItem& item)
{
    // INSERT INTO play_queue (ord, imdb_id, media_kind, season, episode,
    //   title, series_title, episode_title, poster,
    //   info_hash, release_name, resolution, quality_label,
    //   size_bytes, provider, added_at)
    q.addBindValue(item.order);
    q.addBindValue(item.key.imdbId);
    q.addBindValue(item.key.kind == api::MediaKind::Series ? 1 : 0);
    q.addBindValue(item.key.season ? QVariant(*item.key.season) : QVariant());
    q.addBindValue(item.key.episode ? QVariant(*item.key.episode) : QVariant());
    q.addBindValue(nullSafe(item.title));
    q.addBindValue(nullSafe(item.seriesTitle));
    q.addBindValue(nullSafe(item.episodeTitle));
    q.addBindValue(nullSafe(item.poster.isValid()
            ? item.poster.toString()
            : QString()));
    q.addBindValue(nullSafe(item.streamRef.infoHash));
    q.addBindValue(nullSafe(item.streamRef.releaseName));
    q.addBindValue(nullSafe(item.streamRef.resolution));
    q.addBindValue(nullSafe(item.streamRef.qualityLabel));
    q.addBindValue(item.streamRef.sizeBytes
            ? QVariant(*item.streamRef.sizeBytes)
            : QVariant());
    q.addBindValue(nullSafe(item.streamRef.provider));
    const auto added = item.addedAt.isValid()
        ? item.addedAt
        : QDateTime::currentDateTimeUtc();
    q.addBindValue(isoUtc(added));
}

constexpr const char* kInsertSql = R"(
    INSERT INTO play_queue (
        ord, imdb_id, media_kind, season, episode,
        title, series_title, episode_title, poster,
        info_hash, release_name, resolution, quality_label,
        size_bytes, provider, added_at
    ) VALUES (
        ?, ?, ?, ?, ?,
        ?, ?, ?, ?,
        ?, ?, ?, ?,
        ?, ?, ?
    )
)";

} // namespace

PlayQueueStore::PlayQueueStore(Database& db, QObject* parent)
    : QObject(parent)
    , m_db(db)
{
}

PlayQueueStore::~PlayQueueStore() = default;

QList<api::QueueItem> PlayQueueStore::loadAll() const
{
    QList<api::QueueItem> out;
    if (!m_db.isOpen()) {
        return out;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("SELECT ")
            + QString::fromLatin1(kSelectColumns)
            + QStringLiteral(" FROM play_queue ORDER BY ord ASC"))) {
        qCWarning(KINEMA)
            << "PlayQueueStore: loadAll failed:" << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.append(hydrate(q));
    }
    return out;
}

qint64 PlayQueueStore::insert(const api::QueueItem& item)
{
    if (!m_db.isOpen()) {
        return 0;
    }
    auto q = m_db.query();
    q.prepare(QString::fromLatin1(kInsertSql));
    bindInsertValues(q, item);
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "PlayQueueStore: insert failed:" << q.lastError().text();
        return 0;
    }
    return q.lastInsertId().toLongLong();
}

void PlayQueueStore::remove(qint64 id)
{
    if (!m_db.isOpen() || id <= 0) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("DELETE FROM play_queue WHERE id = ?"));
    q.addBindValue(id);
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "PlayQueueStore: remove failed:" << q.lastError().text();
    }
}

void PlayQueueStore::clear()
{
    if (!m_db.isOpen()) {
        return;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("DELETE FROM play_queue"))) {
        qCWarning(KINEMA)
            << "PlayQueueStore: clear failed:" << q.lastError().text();
    }
}

QList<api::QueueItem> PlayQueueStore::replaceAll(
    const QList<api::QueueItem>& items)
{
    if (!m_db.isOpen()) {
        return {};
    }

    auto db = QSqlDatabase::database(m_db.connectionName());
    if (!db.transaction()) {
        qCWarning(KINEMA)
            << "PlayQueueStore: transaction() failed:"
            << db.lastError().text();
        return {};
    }

    QSqlQuery clearQ(db);
    if (!clearQ.exec(QStringLiteral("DELETE FROM play_queue"))) {
        qCWarning(KINEMA)
            << "PlayQueueStore: replaceAll DELETE failed:"
            << clearQ.lastError().text();
        db.rollback();
        return {};
    }

    QList<api::QueueItem> out = items;
    QSqlQuery ins(db);
    ins.prepare(QString::fromLatin1(kInsertSql));
    for (int i = 0; i < out.size(); ++i) {
        auto& it = out[i];
        // Renumber order to its list position so callers don't have
        // to think about gaps.
        it.order = i;
        ins.bindValue(0, it.order);
        ins.bindValue(1, it.key.imdbId);
        ins.bindValue(2, it.key.kind == api::MediaKind::Series ? 1 : 0);
        ins.bindValue(3, it.key.season ? QVariant(*it.key.season) : QVariant());
        ins.bindValue(4, it.key.episode ? QVariant(*it.key.episode) : QVariant());
        ins.bindValue(5, nullSafe(it.title));
        ins.bindValue(6, nullSafe(it.seriesTitle));
        ins.bindValue(7, nullSafe(it.episodeTitle));
        ins.bindValue(8, nullSafe(it.poster.isValid()
                ? it.poster.toString()
                : QString()));
        ins.bindValue(9, nullSafe(it.streamRef.infoHash));
        ins.bindValue(10, nullSafe(it.streamRef.releaseName));
        ins.bindValue(11, nullSafe(it.streamRef.resolution));
        ins.bindValue(12, nullSafe(it.streamRef.qualityLabel));
        ins.bindValue(13, it.streamRef.sizeBytes
                ? QVariant(*it.streamRef.sizeBytes)
                : QVariant());
        ins.bindValue(14, nullSafe(it.streamRef.provider));
        const auto added = it.addedAt.isValid()
            ? it.addedAt
            : QDateTime::currentDateTimeUtc();
        ins.bindValue(15, isoUtc(added));
        if (!ins.exec()) {
            qCWarning(KINEMA)
                << "PlayQueueStore: replaceAll INSERT failed:"
                << ins.lastError().text();
            db.rollback();
            return {};
        }
        it.id = ins.lastInsertId().toLongLong();
        if (!it.addedAt.isValid()) {
            it.addedAt = added;
        }
    }

    if (!db.commit()) {
        qCWarning(KINEMA)
            << "PlayQueueStore: replaceAll commit failed:"
            << db.lastError().text();
        db.rollback();
        return {};
    }
    return out;
}

} // namespace kinema::core
