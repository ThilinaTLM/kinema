// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/PlaybackContext.h"
#include "core/Database.h"
#include "core/HistoryStore.h"

#include <QDateTime>
#include <QSignalSpy>
#include <QTest>

using namespace kinema;

namespace {

api::HistoryEntry makeMovieEntry(const QString& imdb, double pos,
    double dur, const QDateTime& watched = QDateTime::currentDateTimeUtc())
{
    api::HistoryEntry e;
    e.key.kind = api::MediaKind::Movie;
    e.key.imdbId = imdb;
    e.title = QStringLiteral("Movie ") + imdb;
    e.poster = QUrl(QStringLiteral("https://example.com/") + imdb + QStringLiteral(".jpg"));
    e.positionSec = pos;
    e.durationSec = dur;
    e.lastWatchedAt = watched;
    e.lastStream.infoHash = QStringLiteral("hash-") + imdb;
    e.lastStream.releaseName = QStringLiteral("Release.") + imdb;
    e.lastStream.resolution = QStringLiteral("1080p");
    e.lastStream.qualityLabel = QStringLiteral("Torrentio 1080p");
    e.lastStream.sizeBytes = 123456789;
    e.lastStream.provider = QStringLiteral("YTS");
    return e;
}

api::HistoryEntry makeEpisodeEntry(const QString& imdb, int season, int episode,
    double pos, double dur,
    const QDateTime& watched = QDateTime::currentDateTimeUtc())
{
    api::HistoryEntry e;
    e.key.kind = api::MediaKind::Series;
    e.key.imdbId = imdb;
    e.key.season = season;
    e.key.episode = episode;
    e.title = QStringLiteral("Series ") + imdb;
    e.seriesTitle = e.title;
    e.episodeTitle = QStringLiteral("S%1E%2").arg(season).arg(episode);
    e.positionSec = pos;
    e.durationSec = dur;
    e.lastWatchedAt = watched;
    e.lastStream.infoHash = QStringLiteral("hash-%1-%2-%3")
                                .arg(imdb).arg(season).arg(episode);
    e.lastStream.releaseName = QStringLiteral("Release.S%1E%2")
                                   .arg(season).arg(episode);
    e.lastStream.resolution = QStringLiteral("1080p");
    return e;
}

} // namespace

class TstHistoryStore : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_db = std::make_unique<core::Database>(
            QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::HistoryStore>(*m_db);
    }

    void cleanup()
    {
        m_store.reset();
        m_db.reset();
    }

    // ---- Basic upsert + find --------------------------------------------

    void testInsertAndFind()
    {
        const auto e = makeMovieEntry(QStringLiteral("tt1000001"), 100, 5000);
        m_store->record(e);

        const auto got = m_store->find(e.key);
        QVERIFY(got.has_value());
        QCOMPARE(got->key.imdbId, QStringLiteral("tt1000001"));
        QCOMPARE(got->positionSec, 100.0);
        QCOMPARE(got->durationSec, 5000.0);
        QCOMPARE(got->lastStream.infoHash, QStringLiteral("hash-tt1000001"));
        QCOMPARE(got->lastStream.releaseName,
            QStringLiteral("Release.tt1000001"));
        QVERIFY(!got->finished);
    }

    // ---- Progress-only update preserves the stream ref ------------------

    void testProgressTickPreservesStreamRef()
    {
        auto e = makeMovieEntry(QStringLiteral("tt1000002"), 100, 5000);
        m_store->record(e);

        // Simulate a position-observer tick with no stream metadata.
        api::HistoryEntry tick;
        tick.key = e.key;
        tick.positionSec = 250;
        tick.durationSec = 5000;
        tick.lastWatchedAt = QDateTime::currentDateTimeUtc();
        // tick.lastStream is empty on purpose.
        m_store->record(tick);

        const auto got = m_store->find(e.key);
        QVERIFY(got.has_value());
        QCOMPARE(got->positionSec, 250.0);
        // Stream ref survived because tick.lastStream was empty.
        QCOMPARE(got->lastStream.infoHash, QStringLiteral("hash-tt1000002"));
        QCOMPARE(got->lastStream.resolution, QStringLiteral("1080p"));
        // Title also preserved (tick had no title either).
        QCOMPARE(got->title, e.title);
    }

    // ---- Auto-flip finished at threshold --------------------------------

    void testAutoFinishedAtThreshold()
    {
        const auto e = makeMovieEntry(
            QStringLiteral("tt1000003"), /*pos=*/5000, /*dur=*/5000);
        m_store->record(e);

        const auto got = m_store->find(e.key);
        QVERIFY(got.has_value());
        QVERIFY(got->finished);
    }

    void testBelowThresholdStaysInProgress()
    {
        const auto e = makeMovieEntry(
            QStringLiteral("tt1000004"), /*pos=*/4000, /*dur=*/5000);
        m_store->record(e);

        const auto got = m_store->find(e.key);
        QVERIFY(got.has_value());
        QVERIFY(!got->finished);
    }

    void testExplicitFinishedFlagHonoured()
    {
        auto e = makeMovieEntry(
            QStringLiteral("tt1000005"), /*pos=*/100, /*dur=*/5000);
        e.finished = true;
        m_store->record(e);
        const auto got = m_store->find(e.key);
        QVERIFY(got->finished);
    }

    // ---- Continue-Watching excludes finished, dedupes series ------------

    void testContinueWatchingOrderAndDedup()
    {
        const auto now = QDateTime::currentDateTimeUtc();

        // Movie, mid-progress, oldest.
        m_store->record(makeMovieEntry(
            QStringLiteral("tt2000001"), 100, 5000, now.addSecs(-3600)));

        // Series A, two episodes. The later-watched episode should win.
        m_store->record(makeEpisodeEntry(
            QStringLiteral("tt3000001"), 1, 1, 100, 2700,
            now.addSecs(-1800)));
        m_store->record(makeEpisodeEntry(
            QStringLiteral("tt3000001"), 1, 2, 200, 2700,
            now.addSecs(-600)));

        // Series B, single episode, oldest.
        m_store->record(makeEpisodeEntry(
            QStringLiteral("tt4000001"), 1, 1, 50, 2700,
            now.addSecs(-7200)));

        // A finished row must NOT appear in continueWatching.
        m_store->record(makeMovieEntry(
            QStringLiteral("tt5000001"), 5000, 5000, now.addSecs(-300)));

        const auto list = m_store->continueWatching(10);

        // Three buckets (two series imdb_ids + one movie), finished
        // movie excluded.
        QCOMPARE(list.size(), 3);

        // Order: most recent first.
        QCOMPARE(list[0].key.imdbId, QStringLiteral("tt3000001"));
        QCOMPARE(list[0].key.episode.value_or(-1), 2);
        QCOMPARE(list[1].key.imdbId, QStringLiteral("tt2000001"));
        QCOMPARE(list[2].key.imdbId, QStringLiteral("tt4000001"));
    }

    void testContinueWatchingLimit()
    {
        const auto now = QDateTime::currentDateTimeUtc();
        for (int i = 0; i < 5; ++i) {
            m_store->record(makeMovieEntry(
                QStringLiteral("tt9%1").arg(i, 6, 10, QLatin1Char('0')),
                100, 5000, now.addSecs(-60 * (i + 1))));
        }
        QCOMPARE(m_store->continueWatching(3).size(), 3);
    }

    // ---- findLatestForMedia ---------------------------------------------

    void testFindLatestForMediaSeries()
    {
        const auto now = QDateTime::currentDateTimeUtc();
        m_store->record(makeEpisodeEntry(
            QStringLiteral("tt6000001"), 1, 1, 100, 2700,
            now.addSecs(-3600)));
        m_store->record(makeEpisodeEntry(
            QStringLiteral("tt6000001"), 2, 3, 500, 2700,
            now.addSecs(-300)));

        const auto got = m_store->findLatestForMedia(
            api::MediaKind::Series, QStringLiteral("tt6000001"));
        QVERIFY(got.has_value());
        QCOMPARE(got->key.season.value_or(-1), 2);
        QCOMPARE(got->key.episode.value_or(-1), 3);
    }

    // ---- Retention pass --------------------------------------------------

    void testRetentionPassRemovesFinishedOld()
    {
        const auto now = QDateTime::currentDateTimeUtc();

        // Old finished \u2014 should be removed.
        auto oldFinished = makeMovieEntry(
            QStringLiteral("tt7000001"), 5000, 5000, now.addDays(-400));
        m_store->record(oldFinished);

        // Old in-progress \u2014 must NOT be removed.
        m_store->record(makeMovieEntry(
            QStringLiteral("tt7000002"), 100, 5000, now.addDays(-400)));

        // Recent finished \u2014 must NOT be removed.
        m_store->record(makeMovieEntry(
            QStringLiteral("tt7000003"), 5000, 5000, now.addDays(-30)));

        m_store->runRetentionPass();

        QVERIFY(!m_store->find({ api::MediaKind::Movie,
            QStringLiteral("tt7000001"), {}, {} }).has_value());
        QVERIFY(m_store->find({ api::MediaKind::Movie,
            QStringLiteral("tt7000002"), {}, {} }).has_value());
        QVERIFY(m_store->find({ api::MediaKind::Movie,
            QStringLiteral("tt7000003"), {}, {} }).has_value());
    }

    // ---- Remove / clear --------------------------------------------------

    void testRemove()
    {
        auto e = makeMovieEntry(QStringLiteral("tt8000001"), 100, 5000);
        m_store->record(e);
        QVERIFY(m_store->find(e.key).has_value());

        m_store->remove(e.key);
        QVERIFY(!m_store->find(e.key).has_value());
    }

    void testClear()
    {
        m_store->record(makeMovieEntry(
            QStringLiteral("tt8000002"), 100, 5000));
        m_store->record(makeMovieEntry(
            QStringLiteral("tt8000003"), 200, 5000));
        m_store->clear();
        QVERIFY(m_store->continueWatching(10).isEmpty());
    }

    // ---- changed() is coalesced -----------------------------------------

    void testChangedSignalCoalesced()
    {
        QSignalSpy spy(m_store.get(), &core::HistoryStore::changed);
        m_store->record(makeMovieEntry(
            QStringLiteral("tt8100001"), 100, 5000));
        m_store->record(makeMovieEntry(
            QStringLiteral("tt8100002"), 200, 5000));
        m_store->record(makeMovieEntry(
            QStringLiteral("tt8100003"), 300, 5000));

        QVERIFY(spy.wait(200));
        QCOMPARE(spy.count(), 1);
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_store;
};

QTEST_MAIN(TstHistoryStore)
#include "tst_history_store.moc"
