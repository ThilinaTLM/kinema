// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Discover.h"
#include "api/Media.h"
#include "api/TmdbClient.h"
#include "config/BrowseSettings.h"
#include "core/DateWindow.h"
#include "core/HttpError.h"
#include "ui/qml-bridge/BrowseViewModel.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using kinema::api::DiscoverItem;
using kinema::api::DiscoverPageResult;
using kinema::api::DiscoverQuery;
using kinema::api::DiscoverSort;
using kinema::api::MediaKind;
using kinema::api::TmdbClient;
using kinema::api::TmdbGenre;
using kinema::config::BrowseSettings;
using kinema::core::DateWindow;
using kinema::core::HttpError;
using kinema::ui::qml::BrowseViewModel;
using kinema::ui::qml::DiscoverSectionModel;

namespace {

class FakeTmdb : public TmdbClient
{
public:
    FakeTmdb()
        : TmdbClient(nullptr)
    {
    }

    // Canned response for /discover. The view-model uses
    // `result.totalPages` for pagination so populate it explicitly.
    DiscoverPageResult cannedPage;
    QList<TmdbGenre> cannedGenresMovie;
    QList<TmdbGenre> cannedGenresSeries;

    bool throwAuth = false;
    bool throwGeneric = false;

    int discoverCalls = 0;
    int genreCalls = 0;
    DiscoverQuery lastQuery;

    QCoro::Task<DiscoverPageResult> discover(DiscoverQuery q) override
    {
        ++discoverCalls;
        lastQuery = q;
        if (throwAuth) {
            throw HttpError(HttpError::Kind::HttpStatus, 401,
                QStringLiteral("auth"));
        }
        if (throwGeneric) {
            throw HttpError(HttpError::Kind::Network, 0,
                QStringLiteral("network"));
        }
        co_return cannedPage;
    }

    QCoro::Task<QList<TmdbGenre>> genreList(MediaKind kind) override
    {
        ++genreCalls;
        co_return kind == MediaKind::Series
            ? cannedGenresSeries : cannedGenresMovie;
    }
};

DiscoverItem makeItem(int id, MediaKind kind, const QString& title)
{
    DiscoverItem it;
    it.tmdbId = id;
    it.kind = kind;
    it.title = title;
    it.year = 2024;
    it.voteAverage = 7.0;
    return it;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstBrowseViewModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        m_tmpdir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmpdir->isValid());
        m_config = KSharedConfig::openConfig(
            m_tmpdir->filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
        m_settings
            = std::make_unique<BrowseSettings>(m_config, nullptr);
    }

    void cleanup()
    {
        m_settings.reset();
        m_config.reset();
        m_tmpdir.reset();
    }

    void testNotConfiguredHidesResults()
    {
        FakeTmdb tmdb;
        // No setToken → hasToken() is false. refresh() bails before
        // any /discover call.
        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        QVERIFY(!vm.tmdbConfigured());
        vm.refresh();
        drain();
        QCOMPARE(tmdb.discoverCalls, 0);
        QCOMPARE(vm.results()->state(),
            DiscoverSectionModel::State::Empty);
    }

    void testRefreshPopulatesResults()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        DiscoverPageResult result;
        result.items = {
            makeItem(1, MediaKind::Movie, QStringLiteral("A")),
            makeItem(2, MediaKind::Movie, QStringLiteral("B")),
        };
        result.page = 1;
        result.totalPages = 3;
        tmdb.cannedPage = result;

        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        QVERIFY(vm.tmdbConfigured());
        vm.refresh();
        drain();

        QCOMPARE(tmdb.discoverCalls, 1);
        QCOMPARE(vm.results()->state(),
            DiscoverSectionModel::State::Ready);
        QCOMPARE(vm.results()->rowCount(), 2);
        QVERIFY(vm.canLoadMore());
    }

    void testLoadMoreAppendsAndStopsAtTotalPages()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        DiscoverPageResult page1;
        page1.items
            = { makeItem(1, MediaKind::Movie, QStringLiteral("A")) };
        page1.page = 1;
        page1.totalPages = 2;
        tmdb.cannedPage = page1;

        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        vm.refresh();
        drain();

        QCOMPARE(vm.results()->rowCount(), 1);

        DiscoverPageResult page2;
        page2.items
            = { makeItem(2, MediaKind::Movie, QStringLiteral("B")) };
        page2.page = 2;
        page2.totalPages = 2;
        tmdb.cannedPage = page2;

        vm.loadMore();
        drain();

        QCOMPARE(tmdb.discoverCalls, 2);
        QCOMPARE(vm.results()->rowCount(), 2);
        // Page 2 was the last; canLoadMore should be false.
        QVERIFY(!vm.canLoadMore());

        // A second loadMore is a no-op rather than another fetch.
        vm.loadMore();
        drain();
        QCOMPARE(tmdb.discoverCalls, 2);
    }

    void testAuthFailureLatchesPlaceholder()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("bad"));
        tmdb.throwAuth = true;

        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        QSignalSpy authSpy(&vm,
            &BrowseViewModel::authFailedChanged);
        vm.refresh();
        drain();

        QVERIFY(vm.authFailed());
        QCOMPARE(authSpy.count(), 1);
        QVERIFY(!vm.canLoadMore());
    }

    void testGenericErrorPaintsResultModelError()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        tmdb.throwGeneric = true;

        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        vm.refresh();
        drain();
        QVERIFY(!vm.authFailed());
        QCOMPARE(vm.results()->state(),
            DiscoverSectionModel::State::Error);
        QVERIFY(!vm.results()->errorMessage().isEmpty());
    }

    void testFilterChangePersistsToBrowseSettings()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        BrowseViewModel vm(&tmdb, *m_settings, nullptr);

        vm.setSort(static_cast<int>(DiscoverSort::Rating));
        QCOMPARE(static_cast<int>(m_settings->sort()),
            static_cast<int>(DiscoverSort::Rating));

        vm.setMinRatingPct(70);
        QCOMPARE(m_settings->minRatingPct(), 70);

        vm.setHideObscure(false);
        QCOMPARE(m_settings->hideObscure(), false);

        vm.setDateWindow(static_cast<int>(DateWindow::Past3Years));
        QCOMPARE(static_cast<int>(m_settings->dateWindow()),
            static_cast<int>(DateWindow::Past3Years));
    }

    void testKindFlipClearsGenreSelection()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        m_settings->setGenreIds({ 28, 18 });
        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        QCOMPARE(vm.genreIds().size(), 2);

        vm.setKind(static_cast<int>(MediaKind::Series));
        // Kind switch wipes the persisted genre id list because TMDB
        // genre ids don't survive across movie/TV.
        QCOMPARE(vm.genreIds().size(), 0);
        QVERIFY(m_settings->genreIds().isEmpty());
    }

    void testApplyPresetUpdatesKindAndSort()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        BrowseViewModel vm(&tmdb, *m_settings, nullptr);

        QSignalSpy filtersSpy(&vm,
            &BrowseViewModel::filtersChanged);

        vm.applyPreset(static_cast<int>(MediaKind::Series),
            static_cast<int>(DiscoverSort::Rating));

        QCOMPARE(vm.kind(), static_cast<int>(MediaKind::Series));
        QCOMPARE(vm.sort(), static_cast<int>(DiscoverSort::Rating));
        QCOMPARE(vm.minRatingPct(), 0);
        QCOMPARE(vm.hideObscure(), true);
        QVERIFY(filtersSpy.count() >= 1);
    }

    void testActiveChipsReflectNonDefaultState()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        BrowseViewModel vm(&tmdb, *m_settings, nullptr);

        // Default chips: none. The kind is shown by the segmented
        // control in the BrowseFilterBar, not as a chip; only
        // non-default filters surface here.
        const auto baseline = vm.activeChipsList();
        QCOMPARE(baseline.size(), 0);

        vm.setMinRatingPct(70);
        vm.setSort(static_cast<int>(DiscoverSort::Rating));
        const auto chips = vm.activeChipsList();
        // sort + rating
        QCOMPARE(chips.size(), 2);
    }

    void testRemoveChipUndoesFilter()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        vm.setMinRatingPct(70);
        QCOMPARE(vm.activeChipsList().size(), 1);

        // Find the rating chip by `kind == "rating"` and remove it.
        const auto chips = vm.activeChipsList();
        int ratingChipIndex = -1;
        for (int i = 0; i < chips.size(); ++i) {
            if (chips.at(i).toMap()
                    .value(QStringLiteral("kind")).toString()
                == QLatin1String("rating")) {
                ratingChipIndex = i;
                break;
            }
        }
        QVERIFY(ratingChipIndex >= 0);
        vm.removeChip(ratingChipIndex);
        QCOMPARE(vm.minRatingPct(), 0);
    }

    void testStaleResponseDiscarded()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        DiscoverPageResult first;
        first.items
            = { makeItem(1, MediaKind::Movie,
                QStringLiteral("First")) };
        first.page = 1;
        first.totalPages = 1;
        tmdb.cannedPage = first;

        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        vm.refresh();
        // Don't drain — second refresh supersedes the first.
        DiscoverPageResult second;
        second.items
            = { makeItem(2, MediaKind::Movie,
                QStringLiteral("Second")) };
        second.page = 1;
        second.totalPages = 1;
        tmdb.cannedPage = second;
        vm.refresh();
        drain();

        // Only the second response should land.
        QCOMPARE(vm.results()->rowCount(), 1);
        const auto* item = vm.results()->itemAt(0);
        QVERIFY(item != nullptr);
        QCOMPARE(item->title, QStringLiteral("Second"));
    }

    void testActivateRoutesByKind()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        DiscoverPageResult page;
        page.items = {
            makeItem(101, MediaKind::Movie, QStringLiteral("M")),
            makeItem(202, MediaKind::Series, QStringLiteral("S")),
        };
        page.totalPages = 1;
        tmdb.cannedPage = page;

        BrowseViewModel vm(&tmdb, *m_settings, nullptr);
        vm.refresh();
        drain();

        QSignalSpy movieSpy(&vm,
            &BrowseViewModel::openMovieRequested);
        QSignalSpy seriesSpy(&vm,
            &BrowseViewModel::openSeriesRequested);

        vm.activate(0);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(movieSpy.first().at(0).toInt(), 101);

        vm.activate(1);
        QCOMPARE(seriesSpy.count(), 1);
        QCOMPARE(seriesSpy.first().at(0).toInt(), 202);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpdir;
    KSharedConfigPtr m_config;
    std::unique_ptr<BrowseSettings> m_settings;
};

QTEST_MAIN(TstBrowseViewModel)
#include "tst_browse_view_model.moc"
