// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Database.h"

#include "kinema_debug.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QStandardPaths>

#include <atomic>

namespace kinema::core {

namespace {

QString defaultDbPath()
{
    const auto dir = QStandardPaths::writableLocation(
        QStandardPaths::AppDataLocation);
    if (dir.isEmpty()) {
        // Fallback: a temp location so `open()` can still succeed in
        // unusual environments (sandboxed tests, missing XDG vars).
        return QStandardPaths::writableLocation(
                   QStandardPaths::TempLocation)
            + QStringLiteral("/kinema.db");
    }
    return dir + QStringLiteral("/kinema.db");
}

/// Each Database instance gets a unique connection name so multiple
/// instances (tests!) coexist cleanly.
QString makeConnectionName()
{
    static std::atomic<quint64> counter{0};
    const auto n = counter.fetch_add(1, std::memory_order_relaxed);
    return QStringLiteral("kinema_db_%1").arg(n);
}

} // namespace

Database::Database(QObject* parent)
    : Database(defaultDbPath(), parent)
{
}

Database::Database(QString filePath, QObject* parent)
    : QObject(parent)
    , m_path(std::move(filePath))
    , m_connectionName(makeConnectionName())
{
}

Database::~Database()
{
    if (QSqlDatabase::contains(m_connectionName)) {
        {
            auto db = QSqlDatabase::database(m_connectionName, /*open=*/false);
            if (db.isOpen()) {
                db.close();
            }
        }
        QSqlDatabase::removeDatabase(m_connectionName);
    }
}

bool Database::open()
{
    if (m_opened) {
        return true;
    }

    // Ensure the parent directory exists for file-backed DBs.
    if (m_path != QStringLiteral(":memory:")) {
        const QFileInfo fi(m_path);
        const QDir parent = fi.absoluteDir();
        if (!parent.exists() && !parent.mkpath(QStringLiteral("."))) {
            qCWarning(KINEMA)
                << "Database: could not create directory"
                << parent.absolutePath();
            return false;
        }
    }

    const auto tryOpen = [this] {
        auto db = QSqlDatabase::addDatabase(
            QStringLiteral("QSQLITE"), m_connectionName);
        db.setDatabaseName(m_path);
        if (!db.open()) {
            qCWarning(KINEMA)
                << "Database: open failed at" << m_path << "\u2014"
                << db.lastError().text();
            return false;
        }
        // SQLite's open() happily succeeds on garbage files; real
        // validation happens when we issue the first statement
        // (PRAGMA or migration). Treat either failure as "possibly
        // corrupt" and let the caller decide whether to quarantine.
        if (!configurePragmas() || !runMigrations()) {
            db.close();
            return false;
        }
        return true;
    };

    if (tryOpen()) {
        m_opened = true;
        qCInfo(KINEMA) << "Database: opened" << m_path
                       << "schema v" << currentSchemaVersion();
        return true;
    }

    // First attempt failed — tear down the broken connection.
    if (QSqlDatabase::contains(m_connectionName)) {
        QSqlDatabase::removeDatabase(m_connectionName);
    }

    // In-memory DBs have no file to quarantine. If the in-memory
    // path failed, we have nothing to retry against.
    if (m_path == QStringLiteral(":memory:")) {
        return false;
    }

    qCWarning(KINEMA)
        << "Database: initial open failed; quarantining" << m_path;
    quarantineCorruptFile();

    if (tryOpen()) {
        m_opened = true;
        qCInfo(KINEMA) << "Database: opened" << m_path
                       << "schema v" << currentSchemaVersion()
                       << "(after quarantine)";
        return true;
    }

    qCWarning(KINEMA)
        << "Database: reopen after quarantine failed";
    if (QSqlDatabase::contains(m_connectionName)) {
        QSqlDatabase::removeDatabase(m_connectionName);
    }
    return false;
}

QSqlQuery Database::query() const
{
    return QSqlQuery(QSqlDatabase::database(m_connectionName));
}

bool Database::configurePragmas()
{
    auto db = QSqlDatabase::database(m_connectionName);
    QSqlQuery q(db);

    const QStringList pragmas = {
        QStringLiteral("PRAGMA journal_mode=WAL;"),
        QStringLiteral("PRAGMA synchronous=NORMAL;"),
        QStringLiteral("PRAGMA foreign_keys=ON;"),
    };
    for (const auto& p : pragmas) {
        if (!q.exec(p)) {
            qCWarning(KINEMA)
                << "Database: PRAGMA failed:" << p
                << "\u2014" << q.lastError().text();
            return false;
        }
    }
    return true;
}

int Database::currentSchemaVersion() const
{
    auto db = QSqlDatabase::database(m_connectionName);
    QSqlQuery q(db);

    if (!q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='schema_meta'"))) {
        return 0;
    }
    if (!q.next()) {
        return 0;
    }

    if (!q.exec(QStringLiteral(
            "SELECT value FROM schema_meta WHERE key='version'"))) {
        return 0;
    }
    if (!q.next()) {
        return 0;
    }
    bool ok = false;
    const int v = q.value(0).toString().toInt(&ok);
    return ok ? v : 0;
}

bool Database::setSchemaVersion(int version)
{
    auto db = QSqlDatabase::database(m_connectionName);
    QSqlQuery q(db);
    q.prepare(QStringLiteral(
        "INSERT INTO schema_meta (key, value) VALUES ('version', ?) "
        "ON CONFLICT(key) DO UPDATE SET value=excluded.value"));
    q.addBindValue(QString::number(version));
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "Database: setSchemaVersion failed:"
            << q.lastError().text();
        return false;
    }
    return true;
}

bool Database::runMigrations()
{
    auto db = QSqlDatabase::database(m_connectionName);

    const int target = latestSchemaVersion();
    int current = currentSchemaVersion();

    while (current < target) {
        const int next = current + 1;
        if (!db.transaction()) {
            qCWarning(KINEMA)
                << "Database: transaction() failed:"
                << db.lastError().text();
            return false;
        }
        if (!applyMigration(next) || !setSchemaVersion(next)) {
            db.rollback();
            return false;
        }
        if (!db.commit()) {
            qCWarning(KINEMA)
                << "Database: commit failed:" << db.lastError().text();
            db.rollback();
            return false;
        }
        current = next;
    }
    return true;
}

bool Database::applyMigration(int toVersion)
{
    auto db = QSqlDatabase::database(m_connectionName);
    QSqlQuery q(db);

    switch (toVersion) {
    case 1: {
        const QStringList stmts = {
            QStringLiteral(R"(CREATE TABLE IF NOT EXISTS schema_meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            ))"),
            QStringLiteral(R"(CREATE TABLE IF NOT EXISTS history (
                key                  TEXT PRIMARY KEY,
                kind                 TEXT NOT NULL,
                imdb_id              TEXT NOT NULL,
                season               INTEGER,
                episode              INTEGER,
                title                TEXT NOT NULL,
                series_title         TEXT NOT NULL DEFAULT '',
                episode_title        TEXT NOT NULL DEFAULT '',
                poster_url           TEXT NOT NULL DEFAULT '',
                position_sec         REAL NOT NULL DEFAULT 0,
                duration_sec         REAL NOT NULL DEFAULT 0,
                finished             INTEGER NOT NULL DEFAULT 0,
                last_watched_at      TEXT NOT NULL,
                stream_info_hash     TEXT NOT NULL DEFAULT '',
                stream_release_name  TEXT NOT NULL DEFAULT '',
                stream_resolution    TEXT NOT NULL DEFAULT '',
                stream_quality_label TEXT NOT NULL DEFAULT '',
                stream_size_bytes    INTEGER,
                stream_provider      TEXT NOT NULL DEFAULT ''
            ))"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS history_by_last "
                "ON history (last_watched_at DESC)"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS history_by_imdb "
                "ON history (imdb_id, last_watched_at DESC)"),
        };
        for (const auto& s : stmts) {
            if (!q.exec(s)) {
                qCWarning(KINEMA)
                    << "Database: migration v1 failed on" << s
                    << "\u2014" << q.lastError().text();
                return false;
            }
        }
        return true;
    }
    case 2: {
        const QStringList stmts = {
            QStringLiteral(
                "ALTER TABLE history ADD COLUMN audio_lang TEXT NOT NULL DEFAULT ''"),
            QStringLiteral(
                "ALTER TABLE history ADD COLUMN sub_lang TEXT NOT NULL DEFAULT ''"),
        };
        for (const auto& s : stmts) {
            if (!q.exec(s)) {
                qCWarning(KINEMA)
                    << "Database: migration v2 failed on" << s
                    << "\u2014" << q.lastError().text();
                return false;
            }
        }
        return true;
    }
    case 3: {
        const QStringList stmts = {
            QStringLiteral(R"(CREATE TABLE IF NOT EXISTS subtitle_cache (
                file_id            TEXT PRIMARY KEY,
                imdb_id            TEXT NOT NULL,
                season             INTEGER,
                episode            INTEGER,
                language           TEXT NOT NULL,
                language_name      TEXT NOT NULL,
                release_name       TEXT NOT NULL DEFAULT '',
                file_name          TEXT NOT NULL DEFAULT '',
                format             TEXT NOT NULL,
                hearing_impaired   INTEGER NOT NULL DEFAULT 0,
                foreign_parts_only INTEGER NOT NULL DEFAULT 0,
                local_path         TEXT NOT NULL,
                size_bytes         INTEGER NOT NULL,
                added_at           TEXT NOT NULL,
                last_used_at       TEXT NOT NULL
            ))"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS subtitle_cache_by_key "
                "ON subtitle_cache (imdb_id, season, episode, language)"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS subtitle_cache_by_lru "
                "ON subtitle_cache (last_used_at)"),
        };
        for (const auto& s : stmts) {
            if (!q.exec(s)) {
                qCWarning(KINEMA)
                    << "Database: migration v3 failed on" << s
                    << "\u2014" << q.lastError().text();
                return false;
            }
        }
        return true;
    }
    default:
        qCWarning(KINEMA)
            << "Database: no migration registered for version"
            << toVersion;
        return false;
    }
}

void Database::quarantineCorruptFile()
{
    if (m_path == QStringLiteral(":memory:")) {
        return;
    }
    const auto ts = QDateTime::currentDateTimeUtc().toString(
        QStringLiteral("yyyyMMdd-HHmmss"));
    const auto quarantined = m_path + QStringLiteral(".corrupt-") + ts;
    if (QFile::exists(m_path)) {
        if (QFile::rename(m_path, quarantined)) {
            qCWarning(KINEMA)
                << "Database: quarantined corrupt file to" << quarantined;
        } else {
            qCWarning(KINEMA)
                << "Database: could not rename" << m_path
                << "to" << quarantined << "\u2014 removing instead";
            QFile::remove(m_path);
        }
    }
    // Also strip any stale WAL/SHM sidecars that might carry the
    // corruption into the fresh DB.
    QFile::remove(m_path + QStringLiteral("-wal"));
    QFile::remove(m_path + QStringLiteral("-shm"));
}

} // namespace kinema::core
