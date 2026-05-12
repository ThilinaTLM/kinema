// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Library.h"
#include "core/persistence/Database.h"
#include "core/persistence/LibraryStore.h"

#include <QSignalSpy>
#include <QTest>

using namespace kinema;

namespace {

domain::LibraryTitle movieTitle()
{
    domain::LibraryTitle t;
    t.kind = domain::MediaKind::Movie;
    t.imdbId = QStringLiteral("tt1000001");
    t.title = QStringLiteral("Saved Movie");
    t.year = 2026;
    t.poster = QUrl(QStringLiteral("https://example.com/poster.jpg"));
    t.releaseDate = QDate(2026, 5, 1);
    return t;
}

domain::LibraryTitle seriesTitle()
{
    domain::LibraryTitle t;
    t.kind = domain::MediaKind::Series;
    t.imdbId = QStringLiteral("tt2000001");
    t.title = QStringLiteral("Saved Series");
    t.year = 2026;
    return t;
}

domain::LibraryEpisode episode(int season, int number)
{
    domain::LibraryEpisode e;
    e.seriesImdbId = QStringLiteral("tt2000001");
    e.season = season;
    e.episode = number;
    e.title = QStringLiteral("Episode %1").arg(number);
    e.releaseDate = QDate(2026, 5, number);
    return e;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstLibraryStore : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_db = std::make_unique<core::Database>(QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::LibraryStore>(*m_db);
    }

    void cleanup()
    {
        m_store.reset();
        m_db.reset();
    }

    void upsertAndFindMovie()
    {
        QSignalSpy changed(m_store.get(), &core::LibraryStore::changed);
        m_store->upsertTitle(movieTitle());
        drain();
        QCOMPARE(changed.count(), 1);

        const auto got = m_store->find(domain::MediaKind::Movie,
            QStringLiteral("tt1000001"));
        QVERIFY(got.has_value());
        QCOMPARE(got->title, QStringLiteral("Saved Movie"));
        QCOMPARE(got->year.value_or(0), 2026);
        QVERIFY(m_store->contains(domain::MediaKind::Movie,
            QStringLiteral("tt1000001")));

        const auto all = m_store->titles();
        QCOMPARE(all.size(), 1);
        QCOMPARE(all.at(0).imdbId, QStringLiteral("tt1000001"));
    }

    void roundTripsExtendedMetadata()
    {
        // v7 columns: genres, imdb_rating, runtime_minutes, cast_list.
        // Values include comma + semicolon to verify the unit-separator
        // join is splitter-safe.
        auto t = movieTitle();
        t.genres = { QStringLiteral("Sci-Fi & Fantasy"),
            QStringLiteral("Drama, Crime") };
        t.imdbRating = 8.4;
        t.runtimeMinutes = 132;
        t.cast = { QStringLiteral("Actor, Jr."),
            QStringLiteral("Other; Person") };
        m_store->upsertTitle(t);

        const auto got = m_store->find(t.kind, t.imdbId);
        QVERIFY(got.has_value());
        QCOMPARE(got->genres, t.genres);
        QVERIFY(got->imdbRating.has_value());
        QCOMPARE(*got->imdbRating, 8.4);
        QCOMPARE(got->runtimeMinutes.value_or(0), 132);
        QCOMPARE(got->cast, t.cast);
    }

    void extendedMetadataDefaultsAreEmpty()
    {
        // A title saved without genres / rating / runtime / cast
        // (mimicking the pre-v7 row before backfill) should hydrate
        // with empty defaults, not garbage.
        m_store->upsertTitle(movieTitle());
        const auto got = m_store->find(domain::MediaKind::Movie,
            QStringLiteral("tt1000001"));
        QVERIFY(got.has_value());
        QVERIFY(got->genres.isEmpty());
        QVERIFY(!got->imdbRating.has_value());
        QVERIFY(!got->runtimeMinutes.has_value());
        QVERIFY(got->cast.isEmpty());
    }

    void upsertUpdatesAcrossCalls()
    {
        auto t = movieTitle();
        m_store->upsertTitle(t);

        t.title = QStringLiteral("Updated Movie");
        m_store->upsertTitle(t);

        const auto got = m_store->find(t.kind, t.imdbId);
        QVERIFY(got.has_value());
        QCOMPARE(got->title, QStringLiteral("Updated Movie"));
    }

    void episodesRoundTripSorted()
    {
        const auto s = seriesTitle();
        m_store->upsertTitle(s);
        m_store->upsertEpisodes(s.imdbId, { episode(1, 2), episode(1, 1) });

        const auto eps = m_store->episodesForSeries(s.imdbId);
        QCOMPARE(eps.size(), 2);
        QCOMPARE(eps.at(0).episode, 1);
        QCOMPARE(eps.at(1).episode, 2);
    }

    void removeDeletesTitleAndEpisodes()
    {
        const auto s = seriesTitle();
        m_store->upsertTitle(s);
        m_store->upsertEpisodes(s.imdbId,
            { episode(1, 1), episode(1, 2) });

        m_store->remove(domain::MediaKind::Series, s.imdbId);
        QVERIFY(!m_store->find(domain::MediaKind::Series, s.imdbId).has_value());
        QVERIFY(m_store->episodesForSeries(s.imdbId).isEmpty());
        QVERIFY(!m_store->contains(domain::MediaKind::Series, s.imdbId));
    }

    void removeIsNoOpForUnknownTitle()
    {
        QSignalSpy changed(m_store.get(), &core::LibraryStore::changed);
        m_store->remove(domain::MediaKind::Movie,
            QStringLiteral("tt9999999"));
        drain();
        // Nothing was deleted; no change signal should fire.
        QCOMPARE(changed.count(), 0);
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::LibraryStore> m_store;
};

QTEST_MAIN(TstLibraryStore)
#include "tst_library_store.moc"
