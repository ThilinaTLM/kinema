// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Database.h"

#include "kinema_log_app.h"

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
            qCWarning(KINEMA_APP)
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
            qCWarning(KINEMA_APP)
                << "Database: open failed at" << m_path << "\u2014"
                << db.lastError().text();
            return false;
        }
        // SQLite's open() happily succeeds on garbage files; real
        // validation happens when we issue the first statement
        // (PRAGMA or migration). Treat either failure as "possibly
        // corrupt" and let the caller decide whether to quarantine.
        if (!configurePragmas() || !runMigrations()
            || !ensureSupplementalTables()) {
            db.close();
            return false;
        }
        return true;
    };

    if (tryOpen()) {
        m_opened = true;
        qCInfo(KINEMA_APP) << "Database: opened" << m_path
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

    qCWarning(KINEMA_APP)
        << "Database: initial open failed; quarantining" << m_path;
    quarantineCorruptFile();

    if (tryOpen()) {
        m_opened = true;
        qCInfo(KINEMA_APP) << "Database: opened" << m_path
                       << "schema v" << currentSchemaVersion()
                       << "(after quarantine)";
        return true;
    }

    qCWarning(KINEMA_APP)
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
            qCWarning(KINEMA_APP)
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
        qCWarning(KINEMA_APP)
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
            qCWarning(KINEMA_APP)
                << "Database: transaction() failed:"
                << db.lastError().text();
            return false;
        }
        if (!applyMigration(next) || !setSchemaVersion(next)) {
            db.rollback();
            return false;
        }
        if (!db.commit()) {
            qCWarning(KINEMA_APP)
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

    const auto runAll = [&](int version, const QStringList& stmts) {
        for (const auto& s : stmts) {
            if (!q.exec(s)) {
                qCWarning(KINEMA_APP)
                    << "Database: migration v" << version
                    << "failed on" << s
                    << "\u2014" << q.lastError().text();
                return false;
            }
        }
        return true;
    };

    switch (toVersion) {
    case 1:
        return runAll(1, {
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
        });
    case 2:
        return runAll(2, {
            QStringLiteral(
                "ALTER TABLE history ADD COLUMN audio_lang TEXT NOT NULL DEFAULT ''"),
            QStringLiteral(
                "ALTER TABLE history ADD COLUMN sub_lang TEXT NOT NULL DEFAULT ''"),
        });
    case 3:
        return runAll(3, {
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
        });
    case 4:
        return runAll(4, {
            QStringLiteral(R"(CREATE TABLE IF NOT EXISTS play_queue (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                ord           INTEGER NOT NULL,
                imdb_id       TEXT NOT NULL,
                media_kind    INTEGER NOT NULL,
                season        INTEGER,
                episode       INTEGER,
                title         TEXT NOT NULL,
                series_title  TEXT NOT NULL DEFAULT '',
                episode_title TEXT NOT NULL DEFAULT '',
                poster        TEXT NOT NULL DEFAULT '',
                info_hash     TEXT NOT NULL,
                release_name  TEXT NOT NULL DEFAULT '',
                resolution    TEXT NOT NULL DEFAULT '',
                quality_label TEXT NOT NULL DEFAULT '',
                size_bytes    INTEGER,
                provider      TEXT NOT NULL DEFAULT '',
                added_at      TEXT NOT NULL
            ))"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS play_queue_by_ord "
                "ON play_queue (ord)"),
        });
    case 5:
        return runAll(5, {
            QStringLiteral(R"(CREATE TABLE IF NOT EXISTS library_titles (
                kind         TEXT NOT NULL,
                imdb_id      TEXT NOT NULL,
                tmdb_id      INTEGER NOT NULL DEFAULT 0,
                title        TEXT NOT NULL,
                year         INTEGER,
                poster_url   TEXT NOT NULL DEFAULT '',
                backdrop_url TEXT NOT NULL DEFAULT '',
                overview     TEXT NOT NULL DEFAULT '',
                release_date TEXT NOT NULL DEFAULT '',
                active       INTEGER NOT NULL DEFAULT 1,
                added_at     TEXT NOT NULL,
                updated_at   TEXT NOT NULL,
                PRIMARY KEY (kind, imdb_id)
            ))"),
            QStringLiteral(R"(CREATE TABLE IF NOT EXISTS library_episodes (
                series_imdb_id TEXT NOT NULL,
                season         INTEGER NOT NULL,
                episode        INTEGER NOT NULL,
                title          TEXT NOT NULL,
                overview       TEXT NOT NULL DEFAULT '',
                thumbnail_url  TEXT NOT NULL DEFAULT '',
                release_date   TEXT NOT NULL DEFAULT '',
                updated_at     TEXT NOT NULL,
                PRIMARY KEY (series_imdb_id, season, episode)
            ))"),
            QStringLiteral(R"(CREATE TABLE IF NOT EXISTS library_watch_overrides (
                key        TEXT PRIMARY KEY,
                kind       TEXT NOT NULL,
                imdb_id    TEXT NOT NULL,
                season     INTEGER,
                episode    INTEGER,
                state      INTEGER NOT NULL,
                changed_at TEXT NOT NULL
            ))"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS library_titles_by_active_updated "
                "ON library_titles (active, updated_at DESC)"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS library_titles_by_release "
                "ON library_titles (active, release_date)"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS library_episodes_by_series_release "
                "ON library_episodes (series_imdb_id, release_date)"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS library_watch_overrides_by_imdb "
                "ON library_watch_overrides (imdb_id)"),
        });
    case 6:
        // Decouple watched-state tracking from the Library:
        //   * rename `library_watch_overrides` → `watched_overrides`
        //     (the table is no longer library-owned data),
        //   * rebuild `library_titles` without the `active` column
        //     (soft-removes are gone; remove-from-library is
        //     unconditional hard delete).
        return runAll(6, {
            QStringLiteral(
                "ALTER TABLE library_watch_overrides "
                "RENAME TO watched_overrides"),
            QStringLiteral(
                "DROP INDEX IF EXISTS library_watch_overrides_by_imdb"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS watched_overrides_by_imdb "
                "ON watched_overrides (imdb_id)"),
            QStringLiteral(
                "DROP INDEX IF EXISTS library_titles_by_active_updated"),
            QStringLiteral(
                "DROP INDEX IF EXISTS library_titles_by_release"),
            QStringLiteral(R"(CREATE TABLE library_titles_new (
                kind         TEXT NOT NULL,
                imdb_id      TEXT NOT NULL,
                tmdb_id      INTEGER NOT NULL DEFAULT 0,
                title        TEXT NOT NULL,
                year         INTEGER,
                poster_url   TEXT NOT NULL DEFAULT '',
                backdrop_url TEXT NOT NULL DEFAULT '',
                overview     TEXT NOT NULL DEFAULT '',
                release_date TEXT NOT NULL DEFAULT '',
                added_at     TEXT NOT NULL,
                updated_at   TEXT NOT NULL,
                PRIMARY KEY (kind, imdb_id)
            ))"),
            QStringLiteral(R"(INSERT INTO library_titles_new (
                    kind, imdb_id, tmdb_id, title, year, poster_url,
                    backdrop_url, overview, release_date,
                    added_at, updated_at)
                SELECT kind, imdb_id, tmdb_id, title, year, poster_url,
                       backdrop_url, overview, release_date,
                       added_at, updated_at
                FROM library_titles
                WHERE active = 1)"),
            QStringLiteral("DROP TABLE library_titles"),
            QStringLiteral(
                "ALTER TABLE library_titles_new RENAME TO library_titles"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS library_titles_by_updated "
                "ON library_titles (updated_at DESC)"),
            QStringLiteral(
                "CREATE INDEX IF NOT EXISTS library_titles_by_release "
                "ON library_titles (release_date)"),
        });
    case 7:
        // Capture extra Cinemeta fields on saved titles so the
        // Library page can filter offline by genre / rating /
        // runtime without re-fetching meta. `genres` and
        // `cast_list` are joined with the ASCII Unit-Separator
        // (\u001f) so values containing commas / semicolons round
        // trip safely. Pre-existing rows keep the defaults until a
        // lazy backfill (LibraryController::backfillMetadata())
        // refills them on next launch.
        return runAll(7, {
            QStringLiteral(
                "ALTER TABLE library_titles "
                "ADD COLUMN genres TEXT NOT NULL DEFAULT ''"),
            QStringLiteral(
                "ALTER TABLE library_titles "
                "ADD COLUMN imdb_rating REAL"),
            QStringLiteral(
                "ALTER TABLE library_titles "
                "ADD COLUMN runtime_minutes INTEGER"),
            QStringLiteral(
                "ALTER TABLE library_titles "
                "ADD COLUMN cast_list TEXT NOT NULL DEFAULT ''"),
        });
    case 8:
        // Downloads re-architecture: the `download_items` table
        // gains an explicit `mode` column (OnDemand vs Full) and
        // the `state` enum is pruned (Preparing→Resolving,
        // Downloading+Streaming→Active, +Idle). Per AGENTS.md we
        // do not migrate persisted rows; drop the table here and
        // let `ensureSupplementalTables()` recreate it with the
        // new schema on the next call.
        return runAll(8, {
            QStringLiteral(
                "DROP INDEX IF EXISTS download_items_by_state"),
            QStringLiteral(
                "DROP INDEX IF EXISTS "
                "download_items_by_disposition"),
            QStringLiteral(
                "DROP INDEX IF EXISTS "
                "download_items_by_playback_key"),
            QStringLiteral(
                "DROP INDEX IF EXISTS download_items_by_hash"),
            QStringLiteral(
                "DROP TABLE IF EXISTS download_items"),
        });
    default:
        qCWarning(KINEMA_APP)
            << "Database: no migration registered for version"
            << toVersion;
        return false;
    }
}

bool Database::ensureSupplementalTables()
{
    auto db = QSqlDatabase::database(m_connectionName);
    QSqlQuery q(db);

    const QStringList stmts = {
        QStringLiteral(R"(CREATE TABLE IF NOT EXISTS download_items (
            asset_id            TEXT PRIMARY KEY,
            backend_kind        INTEGER NOT NULL,
            state               INTEGER NOT NULL,
            mode                INTEGER NOT NULL DEFAULT 0,
            cache_disposition   INTEGER NOT NULL,
            playback_key        TEXT NOT NULL,
            media_kind          INTEGER NOT NULL,
            imdb_id             TEXT NOT NULL,
            season              INTEGER,
            episode             INTEGER,
            title               TEXT NOT NULL,
            series_title        TEXT NOT NULL DEFAULT '',
            episode_title       TEXT NOT NULL DEFAULT '',
            poster_url          TEXT NOT NULL DEFAULT '',
            info_hash           TEXT NOT NULL DEFAULT '',
            release_name        TEXT NOT NULL DEFAULT '',
            file_index          INTEGER NOT NULL DEFAULT -1,
            file_name_hint      TEXT NOT NULL DEFAULT '',
            quality_label       TEXT NOT NULL DEFAULT '',
            resolution          TEXT NOT NULL DEFAULT '',
            provider            TEXT NOT NULL DEFAULT '',
            expected_size_bytes INTEGER,
            cached_size_bytes   INTEGER NOT NULL DEFAULT 0,
            last_error          TEXT NOT NULL DEFAULT '',
            complete            INTEGER NOT NULL DEFAULT 0,
            local_dir           TEXT NOT NULL DEFAULT '',
            added_at            TEXT NOT NULL,
            updated_at          TEXT NOT NULL,
            last_used_at        TEXT NOT NULL
        ))"),
        QStringLiteral(
            "CREATE INDEX IF NOT EXISTS download_items_by_state "
            "ON download_items (state, updated_at DESC)"),
        QStringLiteral(
            "CREATE INDEX IF NOT EXISTS "
            "download_items_by_mode_state "
            "ON download_items (mode, state, updated_at DESC)"),
        QStringLiteral(
            "CREATE INDEX IF NOT EXISTS download_items_by_disposition "
            "ON download_items (cache_disposition, last_used_at)"),
        QStringLiteral(
            "CREATE INDEX IF NOT EXISTS download_items_by_playback_key "
            "ON download_items (playback_key)"),
        QStringLiteral(
            "CREATE INDEX IF NOT EXISTS download_items_by_hash "
            "ON download_items (info_hash, file_index)"),
    };

    for (const auto& s : stmts) {
        if (!q.exec(s)) {
            qCWarning(KINEMA_APP)
                << "Database: ensureSupplementalTables failed on" << s
                << "\u2014" << q.lastError().text();
            return false;
        }
    }
    return true;
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
            qCWarning(KINEMA_APP)
                << "Database: quarantined corrupt file to" << quarantined;
        } else {
            qCWarning(KINEMA_APP)
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
