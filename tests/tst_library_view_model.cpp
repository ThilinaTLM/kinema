// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Library.h"
#include "controllers/LibraryController.h"
#include "controllers/WatchedController.h"
#include "core/Database.h"
#include "core/LibraryStore.h"
#include "core/WatchedStore.h"
#include "ui/qml-bridge/LibraryListModel.h"
#include "ui/qml-bridge/LibraryViewModel.h"

#include <QSignalSpy>
#include <QTest>

using namespace kinema;
using kinema::ui::qml::LibraryViewModel;

namespace {

api::LibraryTitle title(api::MediaKind kind, const QString& imdb,
    const QString& name, std::optional<QDate> released = QDate(2020, 1, 1))
{
    api::LibraryTitle t;
    t.kind = kind;
    t.imdbId = imdb;
    t.title = name;
    t.releaseDate = released;
    return t;
}

api::LibraryEpisode ep(const QString& imdb, int season, int number,
    std::optional<QDate> released)
{
    api::LibraryEpisode e;
    e.seriesImdbId = imdb;
    e.season = season;
    e.episode = number;
    e.title = QStringLiteral("Episode %1").arg(number);
    e.releaseDate = released;
    return e;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
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

    void emptyStartsOnToWatch()
    {
        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.section(), LibraryViewModel::Section::ToWatch);
        QVERIFY(vm.empty());
        QCOMPARE(vm.model()->rowCount(), 0);
    }

    void movieSectionsRespectReleaseAndWatchedOverride()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Ready Movie")));
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt2"), QStringLiteral("Future Movie"),
            QDate::currentDate().addDays(10)));
        m_watched->setMovieWatched(QStringLiteral("tt1"), true);
        drain();

        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.watchedCount(), 1);
        QCOMPARE(vm.upcomingCount(), 1);

        vm.setSection(LibraryViewModel::Section::Watched);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(vm.model()->data(vm.model()->index(0),
            ui::qml::LibraryListModel::TitleRole).toString(),
            QStringLiteral("Ready Movie"));

        vm.setSection(LibraryViewModel::Section::Upcoming);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(vm.model()->data(vm.model()->index(0),
            ui::qml::LibraryListModel::TitleRole).toString(),
            QStringLiteral("Future Movie"));
    }

    void seriesNextEpisodeAndUpcomingRows()
    {
        const QString imdb = QStringLiteral("ttSeries");
        m_libraryStore->upsertTitle(title(api::MediaKind::Series,
            imdb, QStringLiteral("Saved Show")));
        m_libraryStore->upsertEpisodes(imdb, {
            ep(imdb, 1, 1, QDate(2020, 1, 1)),
            ep(imdb, 1, 2, QDate(2020, 1, 2)),
            ep(imdb, 2, 1, QDate::currentDate().addDays(3)),
        });
        m_watched->setEpisodeWatched(imdb, 1, 1, true);
        drain();

        LibraryViewModel vm(m_library.get(), m_watched.get());
        QCOMPARE(vm.toWatchCount(), 1);
        QCOMPARE(vm.upcomingCount(), 1);

        vm.setSection(LibraryViewModel::Section::ToWatch);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(vm.model()->data(vm.model()->index(0),
            ui::qml::LibraryListModel::SeasonRole).toInt(), 1);
        QCOMPARE(vm.model()->data(vm.model()->index(0),
            ui::qml::LibraryListModel::EpisodeRole).toInt(), 2);

        vm.setSection(LibraryViewModel::Section::Upcoming);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(vm.model()->data(vm.model()->index(0),
            ui::qml::LibraryListModel::SeasonRole).toInt(), 2);
    }

    void watchedStateOutsideLibraryDoesNotChangeLibraryRows()
    {
        // No library titles \u2014 marking some other movie watched
        // through the WatchedController must not surface in the
        // Library page.
        m_watched->setMovieWatched(QStringLiteral("ttOutside"), true);
        drain();

        LibraryViewModel vm(m_library.get(), m_watched.get());
        QVERIFY(vm.empty());
        QCOMPARE(vm.watchedCount(), 0);
    }

    void removeFromLibraryWipesOnlyLibraryRows()
    {
        m_libraryStore->upsertTitle(title(api::MediaKind::Movie,
            QStringLiteral("tt1"), QStringLiteral("Movie")));
        m_watched->setMovieWatched(QStringLiteral("tt1"), true);
        drain();

        LibraryViewModel vm(m_library.get(), m_watched.get());
        vm.setSection(LibraryViewModel::Section::Watched);
        QCOMPARE(vm.model()->rowCount(), 1);
        vm.removeFromLibrary(0);
        drain();

        QVERIFY(vm.empty());
        // Watched override survives so re-adding the title brings it
        // back to the Watched section.
        QVERIFY(m_watched->isMovieWatched(QStringLiteral("tt1")));
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
