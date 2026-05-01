// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Library.h"
#include "core/Database.h"
#include "core/LibraryStore.h"

#include <QSignalSpy>
#include <QTest>

using namespace kinema;

namespace {

api::LibraryTitle movieTitle()
{
    api::LibraryTitle t;
    t.kind = api::MediaKind::Movie;
    t.imdbId = QStringLiteral("tt1000001");
    t.title = QStringLiteral("Saved Movie");
    t.year = 2026;
    t.poster = QUrl(QStringLiteral("https://example.com/poster.jpg"));
    t.releaseDate = QDate(2026, 5, 1);
    return t;
}

api::LibraryTitle seriesTitle()
{
    api::LibraryTitle t;
    t.kind = api::MediaKind::Series;
    t.imdbId = QStringLiteral("tt2000001");
    t.title = QStringLiteral("Saved Series");
    t.year = 2026;
    return t;
}

api::LibraryEpisode episode(int season, int number)
{
    api::LibraryEpisode e;
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

        const auto got = m_store->find(api::MediaKind::Movie,
            QStringLiteral("tt1000001"));
        QVERIFY(got.has_value());
        QVERIFY(got->active);
        QCOMPARE(got->title, QStringLiteral("Saved Movie"));
        QCOMPARE(got->year.value_or(0), 2026);
        QVERIFY(m_store->isActive(api::MediaKind::Movie,
            QStringLiteral("tt1000001")));
    }

    void softRemovePreservesDataAndReaddReactivates()
    {
        auto t = movieTitle();
        m_store->upsertTitle(t);
        m_store->setActive(t.kind, t.imdbId, false);

        auto got = m_store->find(t.kind, t.imdbId);
        QVERIFY(got.has_value());
        QVERIFY(!got->active);
        QVERIFY(!m_store->isActive(t.kind, t.imdbId));

        t.title = QStringLiteral("Updated Movie");
        t.active = true;
        m_store->upsertTitle(t);
        got = m_store->find(t.kind, t.imdbId);
        QVERIFY(got.has_value());
        QVERIFY(got->active);
        QCOMPARE(got->title, QStringLiteral("Updated Movie"));
    }

    void episodesAndOverridesRoundTrip()
    {
        const auto s = seriesTitle();
        m_store->upsertTitle(s);
        m_store->upsertEpisodes(s.imdbId, { episode(1, 2), episode(1, 1) });

        const auto eps = m_store->episodesForSeries(s.imdbId);
        QCOMPARE(eps.size(), 2);
        QCOMPARE(eps.at(0).episode, 1);
        QCOMPARE(eps.at(1).episode, 2);

        api::PlaybackKey key;
        key.kind = api::MediaKind::Series;
        key.imdbId = s.imdbId;
        key.season = 1;
        key.episode = 2;
        QCOMPARE(m_store->watchOverride(key), api::LibraryWatchOverride::None);
        m_store->setWatchOverride(key, api::LibraryWatchOverride::Watched);
        QCOMPARE(m_store->watchOverride(key), api::LibraryWatchOverride::Watched);
        m_store->setWatchOverride(key, api::LibraryWatchOverride::Unwatched);
        QCOMPARE(m_store->watchOverride(key), api::LibraryWatchOverride::Unwatched);
        m_store->clearWatchOverride(key);
        QCOMPARE(m_store->watchOverride(key), api::LibraryWatchOverride::None);
    }

    void hardDeleteRemovesLibraryOwnedRows()
    {
        const auto s = seriesTitle();
        m_store->upsertTitle(s);
        m_store->upsertEpisodes(s.imdbId, { episode(1, 1) });
        api::PlaybackKey key;
        key.kind = api::MediaKind::Series;
        key.imdbId = s.imdbId;
        key.season = 1;
        key.episode = 1;
        m_store->setWatchOverride(key, api::LibraryWatchOverride::Watched);

        m_store->hardDelete(api::MediaKind::Series, s.imdbId);
        QVERIFY(!m_store->find(api::MediaKind::Series, s.imdbId).has_value());
        QVERIFY(m_store->episodesForSeries(s.imdbId).isEmpty());
        QCOMPARE(m_store->watchOverride(key), api::LibraryWatchOverride::None);
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::LibraryStore> m_store;
};

QTEST_MAIN(TstLibraryStore)
#include "tst_library_store.moc"
