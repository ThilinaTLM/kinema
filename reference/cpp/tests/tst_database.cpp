// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/persistence/Database.h"

#include <QDir>
#include <QFile>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema::core;

class TstDatabase : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_tmp = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmp->isValid());
        m_path = m_tmp->filePath(QStringLiteral("kinema.db"));
    }

    void cleanup()
    {
        m_tmp.reset();
    }

    // ---- Opening a fresh file creates the latest schema ------------------

    void testFreshOpenCreatesSchema()
    {
        Database db(m_path, nullptr);
        QVERIFY(db.open());
        QVERIFY(db.isOpen());
        QCOMPARE(db.currentSchemaVersion(), Database::latestSchemaVersion());
        QCOMPARE(db.currentSchemaVersion(), 9);

        // history table must exist and have the key column.
        auto q = db.query();
        QVERIFY(q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='history'")));
        QVERIFY(q.next());

        QVERIFY(q.exec(QStringLiteral("PRAGMA table_info(history)")));
        QStringList columns;
        while (q.next()) {
            columns << q.value(1).toString();
        }
        QVERIFY(columns.contains(QStringLiteral("key")));
        QVERIFY(columns.contains(QStringLiteral("position_sec")));
        QVERIFY(columns.contains(QStringLiteral("stream_info_hash")));
        QVERIFY(columns.contains(QStringLiteral("audio_lang")));
        QVERIFY(columns.contains(QStringLiteral("sub_lang")));

        // subtitle_cache table from v3 migration.
        QVERIFY(q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='subtitle_cache'")));
        QVERIFY(q.next());
        QVERIFY(q.exec(QStringLiteral("PRAGMA table_info(subtitle_cache)")));
        QStringList subColumns;
        while (q.next()) {
            subColumns << q.value(1).toString();
        }
        QVERIFY(subColumns.contains(QStringLiteral("file_id")));
        QVERIFY(subColumns.contains(QStringLiteral("imdb_id")));
        QVERIFY(subColumns.contains(QStringLiteral("language")));
        QVERIFY(subColumns.contains(QStringLiteral("file_name")));
        QVERIFY(subColumns.contains(QStringLiteral("local_path")));
        QVERIFY(subColumns.contains(QStringLiteral("last_used_at")));

        // library tables from v5 migration (moved out of v4),
        // restructured by v6 (no `active` column).
        QVERIFY(q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='library_titles'")));
        QVERIFY(q.next());
        QVERIFY(q.exec(QStringLiteral("PRAGMA table_info(library_titles)")));
        QStringList libColumns;
        while (q.next()) {
            libColumns << q.value(1).toString();
        }
        QVERIFY(libColumns.contains(QStringLiteral("imdb_id")));
        QVERIFY(!libColumns.contains(QStringLiteral("active")));
        // v7 added genres / imdb_rating / runtime_minutes / cast_list
        // so the Library page can filter offline.
        QVERIFY(libColumns.contains(QStringLiteral("genres")));
        QVERIFY(libColumns.contains(QStringLiteral("imdb_rating")));
        QVERIFY(libColumns.contains(QStringLiteral("runtime_minutes")));
        QVERIFY(libColumns.contains(QStringLiteral("cast_list")));

        QVERIFY(q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='library_episodes'")));
        QVERIFY(q.next());

        // v6 renamed library_watch_overrides → watched_overrides.
        QVERIFY(q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='watched_overrides'")));
        QVERIFY(q.next());
        QVERIFY(q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='library_watch_overrides'")));
        QVERIFY(!q.next());

        // download_items is bootstrapped by ensureSupplementalTables()
        // — idempotent CREATE TABLE IF NOT EXISTS, no schema_meta bump.
        QVERIFY(q.exec(QStringLiteral(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name='download_items'")));
        QVERIFY(q.next());
        QVERIFY(q.exec(QStringLiteral("PRAGMA table_info(download_items)")));
        QStringList dlColumns;
        while (q.next()) {
            dlColumns << q.value(1).toString();
        }
        QVERIFY(dlColumns.contains(QStringLiteral("asset_id")));
        QVERIFY(dlColumns.contains(QStringLiteral("backend_kind")));
        QVERIFY(dlColumns.contains(QStringLiteral("state")));
        QVERIFY(dlColumns.contains(QStringLiteral("cache_disposition")));
        QVERIFY(dlColumns.contains(QStringLiteral("playback_key")));
        QVERIFY(dlColumns.contains(QStringLiteral("info_hash")));
        QVERIFY(dlColumns.contains(QStringLiteral("file_index")));
        QVERIFY(dlColumns.contains(QStringLiteral("cached_size_bytes")));
        QVERIFY(dlColumns.contains(QStringLiteral("local_dir")));
        QVERIFY(dlColumns.contains(QStringLiteral("last_used_at")));
    }

    // ---- Reopen is idempotent -------------------------------------------

    void testReopenIdempotent()
    {
        {
            Database db(m_path, nullptr);
            QVERIFY(db.open());
        }
        {
            Database db(m_path, nullptr);
            QVERIFY(db.open());
            QCOMPARE(db.currentSchemaVersion(), 9);
        }
    }

    // ---- WAL mode is actually applied -----------------------------------

    void testWalModeApplied()
    {
        Database db(m_path, nullptr);
        QVERIFY(db.open());
        auto q = db.query();
        QVERIFY(q.exec(QStringLiteral("PRAGMA journal_mode")));
        QVERIFY(q.next());
        QCOMPARE(q.value(0).toString().toLower(), QStringLiteral("wal"));
    }

    // ---- In-memory DB works for tests -----------------------------------

    void testInMemoryDb()
    {
        Database db(QStringLiteral(":memory:"), nullptr);
        QVERIFY(db.open());
        QCOMPARE(db.currentSchemaVersion(), 9);
    }

    // ---- Corrupt file is quarantined and a fresh DB is created ---------

    void testCorruptFileQuarantined()
    {
        // Write garbage to the path so SQLite's header check fails.
        {
            QFile f(m_path);
            QVERIFY(f.open(QIODevice::WriteOnly));
            f.write(QByteArrayLiteral(
                "this is not a valid sqlite database file at all"));
            f.close();
        }

        Database db(m_path, nullptr);
        QVERIFY(db.open());
        QCOMPARE(db.currentSchemaVersion(), 9);

        // A file named *.corrupt-* should now exist next to the fresh DB.
        const QFileInfo fi(m_path);
        const QDir parent = fi.absoluteDir();
        const auto entries = parent.entryList(
            { fi.fileName() + QStringLiteral(".corrupt-*") },
            QDir::Files);
        QVERIFY2(!entries.isEmpty(),
            "expected a quarantined *.corrupt-* file");
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    QString m_path;
};

QTEST_MAIN(TstDatabase)
#include "tst_database.moc"
