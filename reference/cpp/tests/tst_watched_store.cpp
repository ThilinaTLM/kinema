// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/PlaybackContext.h"
#include "core/persistence/Database.h"
#include "core/persistence/WatchedStore.h"

#include <QSignalSpy>
#include <QTest>

using namespace kinema;

namespace {

domain::PlaybackKey movieKey(const QString& imdb)
{
    domain::PlaybackKey k;
    k.kind = domain::MediaKind::Movie;
    k.imdbId = imdb;
    return k;
}

domain::PlaybackKey episodeKey(const QString& imdb, int season, int episode)
{
    domain::PlaybackKey k;
    k.kind = domain::MediaKind::Series;
    k.imdbId = imdb;
    k.season = season;
    k.episode = episode;
    return k;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstWatchedStore : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_db = std::make_unique<core::Database>(
            QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::WatchedStore>(*m_db);
    }

    void cleanup()
    {
        m_store.reset();
        m_db.reset();
    }

    void noOverrideByDefault()
    {
        QCOMPARE(m_store->override(movieKey(QStringLiteral("tt1"))),
            core::WatchedStore::Override::None);
    }

    void setOverrideRoundTrip()
    {
        QSignalSpy changed(m_store.get(),
            &core::WatchedStore::changed);

        const auto k = movieKey(QStringLiteral("tt1"));
        m_store->setOverride(k, core::WatchedStore::Override::Watched);
        drain();
        QCOMPARE(changed.count(), 1);
        QCOMPARE(m_store->override(k),
            core::WatchedStore::Override::Watched);

        m_store->setOverride(k, core::WatchedStore::Override::Unwatched);
        drain();
        QCOMPARE(m_store->override(k),
            core::WatchedStore::Override::Unwatched);

        m_store->clearOverride(k);
        drain();
        QCOMPARE(m_store->override(k),
            core::WatchedStore::Override::None);
    }

    void setOverrideNoneClears()
    {
        const auto k = movieKey(QStringLiteral("tt1"));
        m_store->setOverride(k, core::WatchedStore::Override::Watched);
        m_store->setOverride(k, core::WatchedStore::Override::None);
        QCOMPARE(m_store->override(k),
            core::WatchedStore::Override::None);
    }

    void overridesForImdb()
    {
        const auto imdb = QStringLiteral("ttSeries");
        m_store->setOverride(episodeKey(imdb, 1, 1),
            core::WatchedStore::Override::Watched);
        m_store->setOverride(episodeKey(imdb, 1, 2),
            core::WatchedStore::Override::Unwatched);
        m_store->setOverride(episodeKey(QStringLiteral("ttOther"), 1, 1),
            core::WatchedStore::Override::Watched);

        const auto rows = m_store->overridesForImdb(imdb);
        QCOMPARE(rows.size(), 2);
    }

    void clearAllForImdbWipesEverything()
    {
        const auto imdb = QStringLiteral("ttSeries");
        m_store->setOverride(episodeKey(imdb, 1, 1),
            core::WatchedStore::Override::Watched);
        m_store->setOverride(episodeKey(imdb, 1, 2),
            core::WatchedStore::Override::Watched);
        m_store->setOverride(movieKey(QStringLiteral("ttKept")),
            core::WatchedStore::Override::Watched);

        m_store->clearAllForImdb(imdb);

        QCOMPARE(m_store->override(episodeKey(imdb, 1, 1)),
            core::WatchedStore::Override::None);
        QCOMPARE(m_store->override(episodeKey(imdb, 1, 2)),
            core::WatchedStore::Override::None);
        QCOMPARE(m_store->override(movieKey(QStringLiteral("ttKept"))),
            core::WatchedStore::Override::Watched);
    }

    void changedSignalIsCoalesced()
    {
        QSignalSpy changed(m_store.get(),
            &core::WatchedStore::changed);
        m_store->setOverride(movieKey(QStringLiteral("tt1")),
            core::WatchedStore::Override::Watched);
        m_store->setOverride(movieKey(QStringLiteral("tt2")),
            core::WatchedStore::Override::Watched);
        m_store->setOverride(movieKey(QStringLiteral("tt3")),
            core::WatchedStore::Override::Watched);
        drain();
        QCOMPARE(changed.count(), 1);
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::WatchedStore> m_store;
};

QTEST_MAIN(TstWatchedStore)
#include "tst_watched_store.moc"
