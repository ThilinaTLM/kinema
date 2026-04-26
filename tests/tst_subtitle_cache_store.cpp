// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Database.h"
#include "core/SubtitleCacheStore.h"

#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QStandardPaths>
#include <QTest>

using namespace kinema;

namespace {

core::SubtitleCacheStore::Entry makeEntry(const QString& fileId,
    const QString& imdbId, const QString& language,
    qint64 sizeBytes = 1024,
    std::optional<int> season = std::nullopt,
    std::optional<int> episode = std::nullopt)
{
    core::SubtitleCacheStore::Entry e;
    e.fileId = fileId;
    e.imdbId = imdbId;
    e.season = season;
    e.episode = episode;
    e.language = language;
    e.languageName = QStringLiteral("English");
    e.releaseName = QStringLiteral("Some.Release.1080p");
    e.fileName = QStringLiteral("Some.Release.1080p.eng.srt");
    e.format = QStringLiteral("srt");
    e.localPath = QStringLiteral("/tmp/sub-") + fileId;
    e.sizeBytes = sizeBytes;
    e.addedAt = QDateTime::currentDateTimeUtc();
    e.lastUsedAt = e.addedAt;
    return e;
}

api::PlaybackKey movieKey(const QString& imdb)
{
    api::PlaybackKey k;
    k.kind = api::MediaKind::Movie;
    k.imdbId = imdb;
    return k;
}

api::PlaybackKey episodeKey(const QString& imdb, int s, int e)
{
    api::PlaybackKey k;
    k.kind = api::MediaKind::Series;
    k.imdbId = imdb;
    k.season = s;
    k.episode = e;
    return k;
}

} // namespace

class TstSubtitleCacheStore : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        m_db = std::make_unique<core::Database>(
            QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::SubtitleCacheStore>(*m_db);
    }

    void cleanup()
    {
        m_store.reset();
        m_db.reset();
    }

    // ---- buildLocalPath: ASCII passthrough -------------------------------

    void testBuildLocalPathAscii()
    {
        const auto path = core::SubtitleCacheStore::buildLocalPath(
            QStringLiteral("tt0133093"),
            QStringLiteral("The.Matrix.1999.1080p.eng.srt"),
            QStringLiteral("srt"));
        QVERIFY(path.contains(QStringLiteral("tt0133093/")));
        QVERIFY(path.contains(QStringLiteral("The.Matrix.1999.1080p.eng-")));
        QVERIFY(path.endsWith(QStringLiteral(".srt")));
    }

    // ---- buildLocalPath: Windows-illegal char replacement ----------------

    void testBuildLocalPathReplacesIllegalChars()
    {
        const auto path = core::SubtitleCacheStore::buildLocalPath(
            QStringLiteral("tt0133093"),
            QStringLiteral("Foo<bad>:name?*.srt"),
            QStringLiteral("srt"));
        const QString fileName = QFileInfo(path).fileName();
        QVERIFY(!fileName.contains(QLatin1Char('<')));
        QVERIFY(!fileName.contains(QLatin1Char('>')));
        QVERIFY(!fileName.contains(QLatin1Char(':')));
        QVERIFY(!fileName.contains(QLatin1Char('?')));
        QVERIFY(!fileName.contains(QLatin1Char('*')));
    }

    // ---- buildLocalPath: NFC normalisation -------------------------------

    void testBuildLocalPathNfc()
    {
        // U+00E9 (é, NFC) vs U+0065 + U+0301 (e + combining acute).
        const QString nfd = QString::fromUtf8("Cafe\u0301.eng.srt");
        const auto path = core::SubtitleCacheStore::buildLocalPath(
            QStringLiteral("tt0133093"), nfd, QStringLiteral("srt"));
        const QString fileName = QFileInfo(path).fileName();
        // The NFC form combines into a single code point (é).
        QVERIFY(fileName.contains(QString::fromUtf8("\u00e9")));
    }

    // ---- buildLocalPath: 200-byte names truncated to 120 -----------------

    void testBuildLocalPathTruncates()
    {
        QString longName;
        for (int i = 0; i < 200; ++i) {
            longName.append(QLatin1Char('a'));
        }
        longName.append(QStringLiteral(".srt"));
        const auto path = core::SubtitleCacheStore::buildLocalPath(
            QStringLiteral("tt0133093"), longName, QStringLiteral("srt"));
        const QString fileName = QFileInfo(path).fileName();
        // Should be capped well below the original 200 bytes.
        QVERIFY(fileName.size() <= 200);
        // The base portion (before the timestamp dash) must be <= 120.
        const int dash = fileName.lastIndexOf(QLatin1Char('-'));
        QVERIFY(dash > 0);
        const QString base = fileName.left(dash);
        QVERIFY(base.toUtf8().size() <= 120);
    }

    // ---- buildLocalPath: missing extension + unknown format → srt --------

    void testBuildLocalPathDefaultsToSrt()
    {
        const auto path = core::SubtitleCacheStore::buildLocalPath(
            QStringLiteral("tt0133093"),
            QStringLiteral("no_extension_here"),
            QString {});
        QVERIFY(path.endsWith(QStringLiteral(".srt")));
    }

    // ---- insert / findByFileId / cachedFileIds round-trip ----------------

    void testInsertAndFindByFileId()
    {
        const auto e = makeEntry(QStringLiteral("F1"),
            QStringLiteral("tt0133093"), QStringLiteral("eng"));
        QVERIFY(m_store->insert(e));

        const auto found = m_store->findByFileId(QStringLiteral("F1"));
        QVERIFY(found.has_value());
        QCOMPARE(found->imdbId, QStringLiteral("tt0133093"));
        QCOMPARE(found->language, QStringLiteral("eng"));
        QCOMPARE(found->sizeBytes, qint64{1024});
    }

    void testCachedFileIdsForMovie()
    {
        QVERIFY(m_store->insert(
            makeEntry(QStringLiteral("F1"), QStringLiteral("tt0133093"),
                QStringLiteral("eng"))));
        QVERIFY(m_store->insert(
            makeEntry(QStringLiteral("F2"), QStringLiteral("tt0133093"),
                QStringLiteral("spa"))));
        QVERIFY(m_store->insert(
            makeEntry(QStringLiteral("F3"), QStringLiteral("tt0944947"),
                QStringLiteral("eng"))));

        const auto ids = m_store->cachedFileIds(movieKey(
            QStringLiteral("tt0133093")));
        QCOMPARE(ids.size(), 2);
        QVERIFY(ids.contains(QStringLiteral("F1")));
        QVERIFY(ids.contains(QStringLiteral("F2")));
    }

    void testCachedFileIdsForEpisode()
    {
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F1"),
            QStringLiteral("tt0944947"), QStringLiteral("eng"),
            1024, /*season=*/1, /*episode=*/2)));
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F2"),
            QStringLiteral("tt0944947"), QStringLiteral("eng"),
            1024, /*season=*/1, /*episode=*/3)));
        // Movie row for the same imdb (no season/episode) must NOT
        // bleed into an episode lookup.
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F3"),
            QStringLiteral("tt0944947"), QStringLiteral("eng"))));

        const auto ids = m_store->cachedFileIds(episodeKey(
            QStringLiteral("tt0944947"), 1, 2));
        QCOMPARE(ids.size(), 1);
        QVERIFY(ids.contains(QStringLiteral("F1")));
    }

    // ---- findFor with language filter ------------------------------------

    void testFindForFiltersByLanguage()
    {
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F1"),
            QStringLiteral("tt0133093"), QStringLiteral("eng"))));
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F2"),
            QStringLiteral("tt0133093"), QStringLiteral("spa"))));

        const auto eng = m_store->findFor(movieKey(QStringLiteral("tt0133093")),
            { QStringLiteral("eng") });
        QCOMPARE(eng.size(), 1);
        QCOMPARE(eng.first().fileId, QStringLiteral("F1"));

        const auto all = m_store->findFor(movieKey(QStringLiteral("tt0133093")));
        QCOMPARE(all.size(), 2);
    }

    // ---- Eviction candidates oldest-first --------------------------------

    void testEvictionCandidatesOrder()
    {
        auto e1 = makeEntry(QStringLiteral("F1"),
            QStringLiteral("tt0133093"), QStringLiteral("eng"), 100);
        e1.lastUsedAt = QDateTime(QDate(2024, 1, 1), QTime(0, 0),
            QTimeZone::utc());

        auto e2 = makeEntry(QStringLiteral("F2"),
            QStringLiteral("tt0133093"), QStringLiteral("spa"), 200);
        e2.lastUsedAt = QDateTime(QDate(2024, 1, 2), QTime(0, 0),
            QTimeZone::utc());

        auto e3 = makeEntry(QStringLiteral("F3"),
            QStringLiteral("tt0944947"), QStringLiteral("eng"), 300);
        e3.lastUsedAt = QDateTime(QDate(2024, 1, 3), QTime(0, 0),
            QTimeZone::utc());

        QVERIFY(m_store->insert(e1));
        QVERIFY(m_store->insert(e2));
        QVERIFY(m_store->insert(e3));
        QCOMPARE(m_store->totalSizeBytes(), qint64{600});

        // Need to free 250 bytes; oldest first → F1 (100), F2 (200) sums 300.
        const auto victims = m_store->evictionCandidates(250);
        QCOMPARE(victims.size(), 2);
        QCOMPARE(victims.at(0).fileId, QStringLiteral("F1"));
        QCOMPARE(victims.at(1).fileId, QStringLiteral("F2"));
    }

    // ---- touch bumps last_used_at ---------------------------------------

    void testTouchUpdatesLastUsed()
    {
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F1"),
            QStringLiteral("tt0133093"), QStringLiteral("eng"))));
        const auto before = m_store->findByFileId(QStringLiteral("F1"))
                                ->lastUsedAt;
        QTest::qWait(1100); // ISO-8601 has second granularity
        m_store->touch(QStringLiteral("F1"));
        const auto after = m_store->findByFileId(QStringLiteral("F1"))
                               ->lastUsedAt;
        QVERIFY(after > before);
    }

    // ---- clearAll ---------------------------------------------------------

    void testClearAll()
    {
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F1"),
            QStringLiteral("tt0133093"), QStringLiteral("eng"))));
        QVERIFY(m_store->insert(makeEntry(QStringLiteral("F2"),
            QStringLiteral("tt0944947"), QStringLiteral("eng"))));
        QCOMPARE(m_store->all().size(), 2);
        m_store->clearAll();
        QCOMPARE(m_store->all().size(), 0);
        QCOMPARE(m_store->totalSizeBytes(), qint64{0});
    }

    // ---- Migration is a no-op on already-v3 DB ---------------------------

    void testRepeatedOpenIsNoop()
    {
        // The DB is already at v3 (we opened it in init()). Opening
        // a second store on the same DB connection must not fail.
        core::SubtitleCacheStore second(*m_db);
        QCOMPARE(second.all().size(), 0);
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::SubtitleCacheStore> m_store;
};

QTEST_MAIN(TstSubtitleCacheStore)
#include "tst_subtitle_cache_store.moc"
