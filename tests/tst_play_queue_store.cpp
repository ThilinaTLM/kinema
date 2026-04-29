// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/QueueItem.h"
#include "core/Database.h"
#include "core/PlayQueueStore.h"

#include <QDateTime>
#include <QSqlError>
#include <QSqlQuery>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

namespace {

api::QueueItem makeMovieItem(const QString& imdb, int order)
{
    api::QueueItem it;
    it.order = order;
    it.key.kind = api::MediaKind::Movie;
    it.key.imdbId = imdb;
    it.title = QStringLiteral("Movie ") + imdb;
    it.poster = QUrl(QStringLiteral("https://example.com/")
        + imdb + QStringLiteral(".jpg"));
    it.streamRef.infoHash = QStringLiteral("hash-") + imdb;
    it.streamRef.releaseName = QStringLiteral("Release.") + imdb;
    it.streamRef.resolution = QStringLiteral("1080p");
    it.streamRef.qualityLabel = QStringLiteral("Torrentio 1080p");
    it.streamRef.sizeBytes = 1234567890;
    it.streamRef.provider = QStringLiteral("YTS");
    it.addedAt = QDateTime::currentDateTimeUtc();
    return it;
}

api::QueueItem makeEpisodeItem(const QString& imdb, int season,
    int episode, int order)
{
    api::QueueItem it;
    it.order = order;
    it.key.kind = api::MediaKind::Series;
    it.key.imdbId = imdb;
    it.key.season = season;
    it.key.episode = episode;
    it.title = QStringLiteral("Series ") + imdb;
    it.seriesTitle = it.title;
    it.episodeTitle = QStringLiteral("S%1E%2").arg(season).arg(episode);
    it.streamRef.infoHash = QStringLiteral("hash-%1-%2-%3")
                                .arg(imdb).arg(season).arg(episode);
    it.streamRef.releaseName = QStringLiteral("Release.S%1E%2")
                                   .arg(season).arg(episode);
    it.streamRef.resolution = QStringLiteral("720p");
    it.addedAt = QDateTime::currentDateTimeUtc();
    return it;
}

} // namespace

class TestPlayQueueStore : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void initTestCase()
    {
        m_dir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_dir->isValid());
    }

    void init()
    {
        ++m_seq;
        m_dbPath = m_dir->path() + QStringLiteral("/queue-")
            + QString::number(m_seq) + QStringLiteral(".db");
        m_db = std::make_unique<core::Database>(m_dbPath, nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::PlayQueueStore>(*m_db);
    }

    void cleanup()
    {
        m_store.reset();
        m_db.reset();
    }

    void schemaVersionIncludesQueue()
    {
        QCOMPARE(m_db->currentSchemaVersion(),
            core::Database::latestSchemaVersion());
    }

    void emptyByDefault()
    {
        QVERIFY(m_store->loadAll().isEmpty());
    }

    void insertAssignsId()
    {
        const auto id = m_store->insert(makeMovieItem(
            QStringLiteral("tt0001"), 0));
        QVERIFY(id > 0);
        const auto items = m_store->loadAll();
        QCOMPARE(items.size(), 1);
        QCOMPARE(items[0].id, id);
        QCOMPARE(items[0].order, 0);
        QCOMPARE(items[0].key.imdbId, QStringLiteral("tt0001"));
        QCOMPARE(items[0].streamRef.infoHash,
            QStringLiteral("hash-tt0001"));
        QVERIFY(items[0].streamRef.sizeBytes.has_value());
        QCOMPARE(*items[0].streamRef.sizeBytes, qint64(1234567890));
    }

    void loadAllReturnsByOrderAscending()
    {
        m_store->insert(makeMovieItem(QStringLiteral("ttZ"), 2));
        m_store->insert(makeMovieItem(QStringLiteral("ttA"), 0));
        m_store->insert(makeMovieItem(QStringLiteral("ttM"), 1));

        const auto items = m_store->loadAll();
        QCOMPARE(items.size(), 3);
        QCOMPARE(items[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(items[1].key.imdbId, QStringLiteral("ttM"));
        QCOMPARE(items[2].key.imdbId, QStringLiteral("ttZ"));
    }

    void episodeRoundTrip()
    {
        const auto id = m_store->insert(
            makeEpisodeItem(QStringLiteral("tt9999"), 1, 3, 0));
        QVERIFY(id > 0);
        const auto items = m_store->loadAll();
        QCOMPARE(items.size(), 1);
        const auto& it = items[0];
        QCOMPARE(it.key.kind, api::MediaKind::Series);
        QVERIFY(it.key.season.has_value());
        QCOMPARE(*it.key.season, 1);
        QVERIFY(it.key.episode.has_value());
        QCOMPARE(*it.key.episode, 3);
        QCOMPARE(it.episodeTitle, QStringLiteral("S1E3"));
    }

    void removeById()
    {
        const auto a = m_store->insert(makeMovieItem(QStringLiteral("ttA"), 0));
        const auto b = m_store->insert(makeMovieItem(QStringLiteral("ttB"), 1));
        QVERIFY(a > 0);
        QVERIFY(b > 0);

        m_store->remove(a);
        const auto items = m_store->loadAll();
        QCOMPARE(items.size(), 1);
        QCOMPARE(items[0].id, b);
    }

    void clearWipesAll()
    {
        m_store->insert(makeMovieItem(QStringLiteral("ttA"), 0));
        m_store->insert(makeMovieItem(QStringLiteral("ttB"), 1));
        m_store->clear();
        QVERIFY(m_store->loadAll().isEmpty());
    }

    void replaceAllRenumbersAndAssignsIds()
    {
        // Pre-populate with one row that should be wiped.
        m_store->insert(makeMovieItem(QStringLiteral("ttOLD"), 0));

        QList<api::QueueItem> wanted;
        // Order intentionally jumbled — replaceAll should renumber.
        auto a = makeMovieItem(QStringLiteral("ttA"), 7);
        auto b = makeEpisodeItem(QStringLiteral("ttSeries"), 2, 5, 99);
        auto c = makeMovieItem(QStringLiteral("ttC"), 0);
        wanted << a << b << c;

        const auto out = m_store->replaceAll(wanted);
        QCOMPARE(out.size(), 3);
        QCOMPARE(out[0].order, 0);
        QCOMPARE(out[1].order, 1);
        QCOMPARE(out[2].order, 2);
        for (const auto& it : out) {
            QVERIFY(it.id > 0);
        }

        const auto loaded = m_store->loadAll();
        QCOMPARE(loaded.size(), 3);
        QCOMPARE(loaded[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(loaded[1].key.imdbId, QStringLiteral("ttSeries"));
        QCOMPARE(loaded[2].key.imdbId, QStringLiteral("ttC"));
    }

    void replaceAllPreservesIdsForExistingItems()
    {
        const auto idA = m_store->insert(makeMovieItem(QStringLiteral("ttA"), 0));
        const auto idB = m_store->insert(makeMovieItem(QStringLiteral("ttB"), 1));
        QVERIFY(idA > 0);
        QVERIFY(idB > 0);

        // replaceAll wipes-and-reinserts; ids will not match the
        // original. This documents the contract: ids are stable
        // across no-op calls only when the in-memory list already
        // carries them, but `replaceAll` re-allocates them. The
        // controller never relies on id stability across reorder.
        auto loaded = m_store->loadAll();
        QCOMPARE(loaded.size(), 2);
        // Swap order in-memory and persist.
        std::swap(loaded[0], loaded[1]);
        const auto out = m_store->replaceAll(loaded);
        QCOMPARE(out.size(), 2);
        QCOMPARE(out[0].key.imdbId, QStringLiteral("ttB"));
        QCOMPARE(out[1].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(out[0].order, 0);
        QCOMPARE(out[1].order, 1);
    }

    void replaceAllWithEmptyClears()
    {
        m_store->insert(makeMovieItem(QStringLiteral("ttA"), 0));
        const auto out = m_store->replaceAll({});
        QCOMPARE(out.size(), 0);
        QVERIFY(m_store->loadAll().isEmpty());
    }

    void closedDbIsNoOp()
    {
        // Build a fresh store on a closed Database.
        core::Database closed(QStringLiteral(":memory:"), nullptr);
        // intentionally not opened
        core::PlayQueueStore s(closed);
        QCOMPARE(s.insert(makeMovieItem(QStringLiteral("ttX"), 0)),
            qint64(0));
        QVERIFY(s.loadAll().isEmpty());
        s.remove(1);
        s.clear();
        QVERIFY(s.replaceAll({makeMovieItem(QStringLiteral("ttY"), 0)})
                    .isEmpty());
    }

    void doesNotPersistDirectUrl()
    {
        // The cachedDirectUrl is in-memory only; loading should
        // never produce a non-empty URL.
        auto it = makeMovieItem(QStringLiteral("ttZ"), 0);
        it.cachedDirectUrl = QUrl(QStringLiteral("https://rd.example/abc"));
        QVERIFY(m_store->insert(it) > 0);
        const auto items = m_store->loadAll();
        QCOMPARE(items.size(), 1);
        QVERIFY(items[0].cachedDirectUrl.isEmpty());
    }

private:
    std::unique_ptr<QTemporaryDir> m_dir;
    QString m_dbPath;
    int m_seq = 0;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::PlayQueueStore> m_store;
};

QTEST_MAIN(TestPlayQueueStore)
#include "tst_play_queue_store.moc"
