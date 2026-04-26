// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>

class QSqlQuery;

namespace kinema::core {

/**
 * Owns the single `QSqlDatabase` SQLite connection used by every DAO
 * in the app (HistoryStore today; WatchlistStore / FavoritesStore
 * tomorrow).
 *
 * Responsibilities:
 *   - Resolve the DB path (`QStandardPaths::AppDataLocation/kinema.db`
 *     by default; overridable for tests) and ensure its directory
 *     exists.
 *   - Open the connection with WAL + synchronous=NORMAL +
 *     foreign_keys=ON.
 *   - Bootstrap / migrate the schema through a tiny
 *     `switch(toVersion)` framework rooted in `schema_meta`.
 *   - Quarantine a corrupt file (rename to `kinema.db.corrupt-<ts>`)
 *     so the app starts with a fresh DB instead of crashing.
 *
 * No domain logic lives here. Callers reach the connection via
 * `Database::query()` or `QSqlDatabase::database(connectionName())`.
 */
class Database : public QObject
{
    Q_OBJECT
public:
    /// Uses `QStandardPaths::AppDataLocation/kinema.db`.
    explicit Database(QObject* parent = nullptr);

    /// Overload for tests or non-default locations. Pass the literal
    /// ":memory:" for an in-memory database.
    Database(QString filePath, QObject* parent);

    ~Database() override;

    Database(const Database&) = delete;
    Database& operator=(const Database&) = delete;

    /// Open + migrate to the latest schema. Returns false on an
    /// unrecoverable failure; callers log + proceed with history
    /// effectively disabled (a null / closed connection doesn't
    /// crash the app, it just makes DAO calls no-ops).
    bool open();

    /// True after a successful `open()`.
    bool isOpen() const noexcept { return m_opened; }

    /// Connection name, usable with
    /// `QSqlDatabase::database(connectionName())`.
    QString connectionName() const noexcept { return m_connectionName; }

    /// Resolved on-disk path (or ":memory:").
    QString filePath() const noexcept { return m_path; }

    /// Convenience: a fresh QSqlQuery bound to our connection. Only
    /// call after `open()` returned true.
    QSqlQuery query() const;

    /// Current schema version per `schema_meta.version`. Returns 0
    /// when the table does not yet exist.
    int currentSchemaVersion() const;

    /// Latest schema version this build knows how to produce.
    static int latestSchemaVersion() noexcept { return 3; }

private:
    bool configurePragmas();
    bool runMigrations();
    bool applyMigration(int toVersion);
    bool setSchemaVersion(int version);
    void quarantineCorruptFile();

    QString m_path;
    QString m_connectionName;
    bool m_opened = false;
};

} // namespace kinema::core
