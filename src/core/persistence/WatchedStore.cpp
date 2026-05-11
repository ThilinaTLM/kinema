// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/persistence/WatchedStore.h"

#include "core/persistence/Database.h"
#include "core/persistence/SqlUtil.h"
#include "kinema_log_db.h"

#include <QDateTime>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QTimer>
#include <QVariant>

namespace kinema::core {

namespace {

using sql::isoUtc;

QString mediaKindToDb(domain::MediaKind k)
{
    return k == domain::MediaKind::Movie
        ? QStringLiteral("movie")
        : QStringLiteral("series");
}

WatchedStore::Override fromDb(int state)
{
    switch (state) {
    case 1: return WatchedStore::Override::Watched;
    case 2: return WatchedStore::Override::Unwatched;
    default: return WatchedStore::Override::None;
    }
}

bool isValid(WatchedStore::Override state)
{
    return state == WatchedStore::Override::Watched
        || state == WatchedStore::Override::Unwatched;
}

} // namespace

WatchedStore::WatchedStore(Database& db, QObject* parent)
    : QObject(parent)
    , m_db(db)
{
}

WatchedStore::~WatchedStore() = default;

WatchedStore::Override WatchedStore::override(
    const domain::PlaybackKey& key) const
{
    if (!m_db.isOpen() || !key.isValid()) {
        return Override::None;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "SELECT state FROM watched_overrides WHERE key = ?"));
    q.addBindValue(key.storageKey());
    if (!q.exec() || !q.next()) {
        return Override::None;
    }
    return fromDb(q.value(0).toInt());
}

QHash<QString, WatchedStore::Override>
WatchedStore::overridesForImdb(const QString& imdbId) const
{
    QHash<QString, Override> out;
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return out;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "SELECT key, state FROM watched_overrides WHERE imdb_id = ?"));
    q.addBindValue(imdbId);
    if (!q.exec()) {
        qCWarning(KINEMA_DB) << "WatchedStore: overridesForImdb failed:"
                          << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.insert(q.value(0).toString(), fromDb(q.value(1).toInt()));
    }
    return out;
}

void WatchedStore::setOverride(const domain::PlaybackKey& key, Override state)
{
    if (!m_db.isOpen() || !key.isValid()) {
        return;
    }
    if (!isValid(state)) {
        clearOverride(key);
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(R"(
        INSERT INTO watched_overrides (
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
        qCWarning(KINEMA_DB) << "WatchedStore: setOverride failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void WatchedStore::clearOverride(const domain::PlaybackKey& key)
{
    if (!m_db.isOpen() || !key.isValid()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "DELETE FROM watched_overrides WHERE key = ?"));
    q.addBindValue(key.storageKey());
    if (!q.exec()) {
        qCWarning(KINEMA_DB) << "WatchedStore: clearOverride failed:"
                          << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        scheduleChanged();
    }
}

void WatchedStore::clearAllForImdb(const QString& imdbId)
{
    if (!m_db.isOpen() || imdbId.isEmpty()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "DELETE FROM watched_overrides WHERE imdb_id = ?"));
    q.addBindValue(imdbId);
    if (!q.exec()) {
        qCWarning(KINEMA_DB) << "WatchedStore: clearAllForImdb failed:"
                          << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        scheduleChanged();
    }
}

void WatchedStore::scheduleChanged()
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
