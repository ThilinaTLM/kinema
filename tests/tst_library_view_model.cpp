// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Library.h"
#include "controllers/LibraryController.h"
#include "controllers/WatchedController.h"
#include "core/Database.h"
#include "core/LibraryStore.h"
#include "core/WatchedStore.h"
#include "ui/qml-bridge/LibraryListModel.h"
#include "ui/qml-bridge/LibraryRailModel.h"
#include "ui/qml-bridge/LibraryViewModel.h"

#include <QSignalSpy>
#include <QTest>

using namespace kinema;
using kinema::ui::qml::LibraryListModel;
using kinema::ui::qml::LibraryRailModel;
using kinema::ui::qml::LibraryViewModel;

namespace {

api::LibraryTitle title(api::MediaKind kind, const QString& imdb,
    const QString& name,
    std::optional<QDate> released = QDate(2020, 1, 1),
    QStringList genres = {},
    std::optional<double> rating = std::nullopt)
{
    api::LibraryTitle t;
    t.kind = kind;
    t.imdbId = imdb;
    t.title = name;
    t.releaseDate = released;
    t.addedAt = QDateTime::currentDateTimeUtc();
    t.genres = std::move(genres);
    t.imdbRating = rating;
    return t;
}

api::LibraryEpisode ep(const QString& imdb, int season, int number,
    std::optional<QDate> released, const QString& epTitle = {})
{
    api::LibraryEpisode e;
    e.seriesImdbId = imdb;
    e.season = season;
    e.episode = number;
    e.title = epTitle.isEmpty()
        ? QStringLiteral("Episode %1").arg(number) : epTitle;
    e.releaseDate = released;
    return e;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

QString roleString(const QAbstractItemModel* m, int row, int role)
{
    return m->data(m->index(row, 0), role).toString();
}

QStringList collectTitles(const QAbstractItemModel* m, int role)
{
    QStringList out;
    for (int i = 0; i < m->rowCount(); ++i) {
        out << roleString(m, i, role);
    }
    return out;
}

} // namespace

class TstLibraryViewModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_db = std::make_unique<core::Database>(QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_libraryStore = std::make_unique<core::LibraryStore>(*m_db);
        m_watchedStore = std::make_unique<core::WatchedStore>(*m_db);
        m_library = std::make_unique<controllers::LibraryController>(
            *m_libraryStore);
        m_watched = std::make_unique<controllers::WatchedController>(
            *m_watchedStore, /*history=*/nullptr);
    }

    void cleanup()
    {
        m_watched.reset();
        m_library.reset();
        m_watchedStore.reset();
        m_libraryStore.reset();
        m_db.reset();
    }

    // ---- defaults ---------------------------------------------------

    void emptyLibraryReportsEmpty()
    {
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QVERIFY(vm.libraryEmpty());
        QVERIFY(vm.empty());
        QCOMPARE(vm.totalCount(), 0);
        QCOMPARE(vm.kind(), LibraryViewModel::KindFilter::All);
        QCOMPARE(vm.status(), LibraryViewModel::StatusFilter::All);
        QCOMPARE(vm.sort(), LibraryViewModel::SortMode::RecentlyAdded);
        QVERIFY(!vm.filtersActive());
    }

    void defaultsShowAllEntries()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Movie One")));
        m_libraryStore->upsertTitle(title(api::MediaKind::Series,
            QStringLiteral("tt2"), QStringLiteral("Series Two")));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.totalCount(), 2);
        QCOMPARE(vm.model()->rowCount(), 2);
        QVERIFY(!vm.libraryEmpty());
    }

    // ---- filter axes ------------------------------------------------

    void kindFilterRestrictsToKind()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Movie One")));
        m_libraryStore->upsertTitle(title(api::MediaKind::Series,
            QStringLiteral("tt2"), QStringLiteral("Series Two")));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());

        vm.setKind(LibraryViewModel::KindFilter::Movies);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(roleString(vm.model(), 0, LibraryListModel::TitleRole),
            QStringLiteral("Movie One"));

        vm.setKind(LibraryViewModel::KindFilter::Series);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(roleString(vm.model(), 0, LibraryListModel::TitleRole),
            QStringLiteral("Series Two"));
    }

    // Note: the Continue status path requires a HistoryController
    // to source resume entries. That integration is exercised
    // end-to-end through the page tests; the VM's filter dispatch
    // is just a comparison against the resolved status enum.

    void statusFilterUpcomingForMovie()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Released"),
            QDate(2020, 1, 1)));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Future"),
            QDate::currentDate().addDays(20)));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setStatus(LibraryViewModel::StatusFilter::Upcoming);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(roleString(vm.model(), 0, LibraryListModel::TitleRole),
            QStringLiteral("Future"));
    }

    void statusFilterWatchedSeries()
    {
        const QString imdb = QStringLiteral("ttSeries");
        m_libraryStore->upsertTitle(title(api::MediaKind::Series,
            imdb, QStringLiteral("Done Show")));
        m_libraryStore->upsertEpisodes(imdb, {
            ep(imdb, 1, 1, QDate(2020, 1, 1)),
            ep(imdb, 1, 2, QDate(2020, 1, 8)),
        });
        m_watched->setEpisodeWatched(imdb, 1, 1, true);
        m_watched->setEpisodeWatched(imdb, 1, 2, true);
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setStatus(LibraryViewModel::StatusFilter::Watched);
        QCOMPARE(vm.model()->rowCount(), 1);
    }

    void hideWatchedDropsWatchedEntries()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Done")));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Pending")));
        m_watched->setMovieWatched(QStringLiteral("tt1"), true);
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setHideWatched(true);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(roleString(vm.model(), 0, LibraryListModel::TitleRole),
            QStringLiteral("Pending"));
    }

    void minRatingExcludesUnratedAndLowRated()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("High"),
            QDate(2020, 1, 1), {}, 8.5));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Low"),
            QDate(2020, 1, 1), {}, 5.0));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt3"), QStringLiteral("Unrated"),
            QDate(2020, 1, 1), {}, std::nullopt));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setMinRatingPct(70);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(roleString(vm.model(), 0, LibraryListModel::TitleRole),
            QStringLiteral("High"));
    }

    void genreFilterMatchesAnyOf()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("SciFi"),
            QDate(2020, 1, 1),
            { QStringLiteral("Sci-Fi"), QStringLiteral("Drama") }));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Comedy"),
            QDate(2020, 1, 1), { QStringLiteral("Comedy") }));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.availableGenres().size(), 3);

        // Resolve the id for "Sci-Fi".
        int sciFiId = -1;
        for (const auto& v : vm.availableGenres()) {
            const auto m = v.toMap();
            if (m.value(QStringLiteral("name")).toString()
                == QStringLiteral("Sci-Fi")) {
                sciFiId = m.value(QStringLiteral("id")).toInt();
            }
        }
        QVERIFY(sciFiId >= 0);

        vm.setGenreIds({ sciFiId });
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(roleString(vm.model(), 0, LibraryListModel::TitleRole),
            QStringLiteral("SciFi"));
    }

    // ---- sort -------------------------------------------------------

    void sortByTitleIsLocaleAware()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Beta")));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Alpha")));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setSort(LibraryViewModel::SortMode::Title);
        QCOMPARE(collectTitles(vm.model(), LibraryListModel::TitleRole),
            QStringList({ QStringLiteral("Alpha"),
                QStringLiteral("Beta") }));
    }

    void sortByRatingPushesUnratedDown()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("None"),
            QDate(2020, 1, 1), {}, std::nullopt));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Mid"),
            QDate(2020, 1, 1), {}, 6.5));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt3"), QStringLiteral("Top"),
            QDate(2020, 1, 1), {}, 9.0));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setSort(LibraryViewModel::SortMode::Rating);
        QCOMPARE(collectTitles(vm.model(), LibraryListModel::TitleRole),
            QStringList({ QStringLiteral("Top"),
                QStringLiteral("Mid"),
                QStringLiteral("None") }));
    }

    // ---- smart rails ------------------------------------------------

    void upNextRailIsPerSeriesNextEpisode()
    {
        const QString imdb = QStringLiteral("ttUpNext");
        m_libraryStore->upsertTitle(title(api::MediaKind::Series,
            imdb, QStringLiteral("Saved Show")));
        m_libraryStore->upsertEpisodes(imdb, {
            ep(imdb, 1, 1, QDate(2020, 1, 1)),
            ep(imdb, 1, 2, QDate(2020, 1, 8)),
            ep(imdb, 1, 3, QDate(2020, 1, 15)),
        });
        m_watched->setEpisodeWatched(imdb, 1, 1, true);
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.upNextModel()->rowCount(), 1);
        QCOMPARE(vm.upNextModel()->data(
            vm.upNextModel()->index(0, 0), LibraryRailModel::EpisodeRole)
                .toInt(), 2);
    }

    void airingSoonRailUsesUpcomingHorizon()
    {
        const QString imdb = QStringLiteral("ttAiring");
        m_libraryStore->upsertTitle(title(api::MediaKind::Series,
            imdb, QStringLiteral("Currently Airing")));
        m_libraryStore->upsertEpisodes(imdb, {
            ep(imdb, 1, 1, QDate(2020, 1, 1)),
            ep(imdb, 1, 2, QDate::currentDate().addDays(5)),
        });
        m_watched->setEpisodeWatched(imdb, 1, 1, true);
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.airingSoonModel()->rowCount(), 1);
    }

    void airingSoonRailExcludesFarFutureEpisodes()
    {
        const QString imdb = QStringLiteral("ttFar");
        m_libraryStore->upsertTitle(title(api::MediaKind::Series,
            imdb, QStringLiteral("Far Off")));
        m_libraryStore->upsertEpisodes(imdb, {
            ep(imdb, 1, 1, QDate::currentDate().addDays(120)),
        });
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.airingSoonModel()->rowCount(), 0);
    }

    void comingUpRailHasFutureMovies()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Released"),
            QDate(2020, 1, 1)));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Soon"),
            QDate::currentDate().addDays(30)));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.comingUpModel()->rowCount(), 1);
        QCOMPARE(vm.comingUpModel()->data(
            vm.comingUpModel()->index(0, 0), LibraryRailModel::TitleRole)
                .toString(), QStringLiteral("Soon"));
    }

    // ---- behaviours preserved from previous VM ----------------------

    void watchedStateOutsideLibraryDoesNotChangeLibraryRows()
    {
        m_watched->setMovieWatched(QStringLiteral("ttOutside"), true);
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QVERIFY(vm.libraryEmpty());
        QCOMPARE(vm.totalCount(), 0);
    }

    void removeFromLibraryWipesOnlyLibraryRows()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Movie")));
        m_watched->setMovieWatched(QStringLiteral("tt1"), true);
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setStatus(LibraryViewModel::StatusFilter::Watched);
        QCOMPARE(vm.model()->rowCount(), 1);
        vm.removeFromLibrary(0);
        drain();
        QVERIFY(vm.libraryEmpty());
        QVERIFY(m_watched->isMovieWatched(QStringLiteral("tt1")));
    }

    // ---- reset filters ----------------------------------------------

    void resetFiltersClearsEverything()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Anything")));
        drain();
        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setKind(LibraryViewModel::KindFilter::Series);
        vm.setStatus(LibraryViewModel::StatusFilter::Watched);
        vm.setMinRatingPct(80);
        vm.setHideWatched(true);
        QVERIFY(vm.filtersActive());
        vm.resetFilters();
        QVERIFY(!vm.filtersActive());
        QCOMPARE(vm.kind(), LibraryViewModel::KindFilter::All);
        QCOMPARE(vm.status(), LibraryViewModel::StatusFilter::All);
        QCOMPARE(vm.minRatingPct(), 0);
        QVERIFY(!vm.hideWatched());
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::LibraryStore> m_libraryStore;
    std::unique_ptr<core::WatchedStore> m_watchedStore;
    std::unique_ptr<controllers::LibraryController> m_library;
    std::unique_ptr<controllers::WatchedController> m_watched;
};

QTEST_MAIN(TstLibraryViewModel)
#include "tst_library_view_model.moc"
