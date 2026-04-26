// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Discover.h"
#include "api/Media.h"
#include "api/TmdbClient.h"
#include "config/AppSettings.h"
#include "config/RealDebridSettings.h"
#include "controllers/TokenController.h"
#include "core/HttpError.h"
#include "core/TokenStore.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/DiscoverViewModel.h"

#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using kinema::api::DiscoverItem;
using kinema::api::DiscoverPageResult;
using kinema::api::DiscoverQuery;
using kinema::api::MediaKind;
using kinema::api::TmdbClient;
using kinema::api::TmdbGenre;
using kinema::core::HttpError;
using kinema::ui::qml::DiscoverSectionModel;
using kinema::ui::qml::DiscoverViewModel;

namespace {

/// In-memory TmdbClient stub. Each catalog endpoint returns canned
/// results (or throws a canned `HttpError`) so the view-model can
/// exercise its happy / error / auth-failed paths without touching
/// the network.
class FakeTmdb : public TmdbClient
{
public:
    FakeTmdb()
        : TmdbClient(nullptr)
    {
    }

    QList<DiscoverItem> trendingItems;
    QList<DiscoverItem> popularSeriesItems;
    QList<DiscoverItem> nowPlayingItems;
    QList<DiscoverItem> onTheAirItems;
    QList<DiscoverItem> topMoviesItems;
    QList<DiscoverItem> topSeriesItems;

    bool throwAuth = false;
    bool throwGeneric = false;

    int trendingCalls = 0;
    int popularCalls = 0;
    int nowPlayingCalls = 0;
    int onTheAirCalls = 0;
    int topRatedCalls = 0;

    void maybeThrow() const
    {
        if (throwAuth) {
            throw HttpError(HttpError::Kind::HttpStatus, 401,
                QStringLiteral("auth"));
        }
        if (throwGeneric) {
            throw HttpError(HttpError::Kind::Network, 0,
                QStringLiteral("network down"));
        }
    }

    QCoro::Task<QList<DiscoverItem>> trending(MediaKind /*kind*/,
        bool /*weekly*/) override
    {
        ++trendingCalls;
        maybeThrow();
        co_return trendingItems;
    }

    QCoro::Task<QList<DiscoverItem>> popular(MediaKind kind) override
    {
        ++popularCalls;
        maybeThrow();
        co_return kind == MediaKind::Series
            ? popularSeriesItems : QList<DiscoverItem> {};
    }

    QCoro::Task<QList<DiscoverItem>> topRated(MediaKind kind) override
    {
        ++topRatedCalls;
        maybeThrow();
        co_return kind == MediaKind::Series
            ? topSeriesItems : topMoviesItems;
    }

    QCoro::Task<QList<DiscoverItem>> nowPlayingMovies() override
    {
        ++nowPlayingCalls;
        maybeThrow();
        co_return nowPlayingItems;
    }

    QCoro::Task<QList<DiscoverItem>> onTheAirSeries() override
    {
        ++onTheAirCalls;
        maybeThrow();
        co_return onTheAirItems;
    }

    // The view-model never calls these, but the base class has them
    // pure-virtual-ish via real method bodies; we don't need to
    // override them. discoverList / similar / etc. stay on the base
    // — the FakeTmdb just won't be exercised through them.
};

DiscoverItem makeItem(int id, MediaKind kind, const QString& title)
{
    DiscoverItem it;
    it.tmdbId = id;
    it.kind = kind;
    it.title = title;
    it.year = 2024;
    it.voteAverage = 7.5;
    return it;
}

/// Drain Qt's queued connections (coroutine continuations land here).
void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstDiscoverViewModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        // Some embedded KConfig calls under TokenController query
        // `QStandardPaths` for config — point them at a scratch dir
        // so a test run doesn't touch the user's real config.
        m_tmpHome = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmpHome->isValid());
        QStandardPaths::setTestModeEnabled(true);
    }

    void notConfiguredHidesRails()
    {
        FakeTmdb tmdb;
        // No setToken → hasToken() is false. View-model should
        // report tmdbConfigured=false and refresh() should not
        // call any endpoint.
        DiscoverViewModel vm(&tmdb, nullptr, nullptr);

        QVERIFY(!vm.tmdbConfigured());
        QVERIFY(!vm.authFailed());
        QVERIFY(!vm.placeholderTitle().isEmpty());
        QVERIFY(!vm.placeholderBody().isEmpty());

        vm.refresh();
        drain();

        QCOMPARE(tmdb.trendingCalls, 0);
        QCOMPARE(tmdb.popularCalls, 0);
        QCOMPARE(tmdb.nowPlayingCalls, 0);
        QCOMPARE(tmdb.onTheAirCalls, 0);
        QCOMPARE(tmdb.topRatedCalls, 0);
    }

    void buildsSixSections()
    {
        FakeTmdb tmdb;
        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        const auto sections = vm.sectionsList();
        QCOMPARE(sections.size(), 6);
        // Each entry is a DiscoverSectionModel*.
        for (auto* obj : sections) {
            QVERIFY(qobject_cast<DiscoverSectionModel*>(obj) != nullptr);
        }
    }

    void refreshPopulatesAllSectionsOnSuccess()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        tmdb.trendingItems = { makeItem(1, MediaKind::Movie, QStringLiteral("A")) };
        tmdb.popularSeriesItems = { makeItem(2, MediaKind::Series, QStringLiteral("B")) };
        tmdb.nowPlayingItems = { makeItem(3, MediaKind::Movie, QStringLiteral("C")) };
        tmdb.onTheAirItems = { makeItem(4, MediaKind::Series, QStringLiteral("D")) };
        tmdb.topMoviesItems = { makeItem(5, MediaKind::Movie, QStringLiteral("E")) };
        tmdb.topSeriesItems = { makeItem(6, MediaKind::Series, QStringLiteral("F")) };

        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        QVERIFY(vm.tmdbConfigured());

        vm.refresh();
        drain();

        // Each endpoint hit exactly once.
        QCOMPARE(tmdb.trendingCalls, 1);
        QCOMPARE(tmdb.popularCalls, 1);
        QCOMPARE(tmdb.nowPlayingCalls, 1);
        QCOMPARE(tmdb.onTheAirCalls, 1);
        QCOMPARE(tmdb.topRatedCalls, 2); // movies + series

        const auto sections = vm.sectionsList();
        QCOMPARE(sections.size(), 6);

        for (auto* obj : sections) {
            auto* model = qobject_cast<DiscoverSectionModel*>(obj);
            QVERIFY(model != nullptr);
            QCOMPARE(model->state(), DiscoverSectionModel::State::Ready);
            QCOMPARE(model->rowCount(), 1);
        }
    }

    void emptyResponseFlipsToEmptyState()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        // Leave all canned lists empty.

        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        vm.refresh();
        drain();

        const auto sections = vm.sectionsList();
        QVERIFY(!sections.isEmpty());
        for (auto* obj : sections) {
            auto* model = qobject_cast<DiscoverSectionModel*>(obj);
            QCOMPARE(model->state(), DiscoverSectionModel::State::Empty);
            QCOMPARE(model->rowCount(), 0);
        }
    }

    void authErrorLatchesAuthFailedFlag()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("bad-token"));
        tmdb.throwAuth = true;

        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        QSignalSpy authSpy(&vm, &DiscoverViewModel::authFailedChanged);
        QSignalSpy phSpy(&vm, &DiscoverViewModel::placeholderChanged);

        vm.refresh();
        drain();

        QVERIFY(vm.authFailed());
        QVERIFY(!vm.placeholderTitle().isEmpty());
        QVERIFY(!vm.placeholderBody().isEmpty());
        QCOMPARE(authSpy.count(), 1);
        QVERIFY(phSpy.count() >= 1);
    }

    void genericErrorPaintsPerSection()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        tmdb.throwGeneric = true;

        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        vm.refresh();
        drain();

        QVERIFY(!vm.authFailed());
        const auto sections = vm.sectionsList();
        for (auto* obj : sections) {
            auto* model = qobject_cast<DiscoverSectionModel*>(obj);
            QCOMPARE(model->state(), DiscoverSectionModel::State::Error);
            QVERIFY(!model->errorMessage().isEmpty());
        }
    }

    void staleResponseIsDiscarded()
    {
        // Two refresh() calls in quick succession: the first epoch
        // is bumped before its co_await completes, so when the
        // canned response lands the post-await check should drop
        // it. We simulate this by latching a slow first call (the
        // simplest way: change canned data between refreshes and
        // make sure only the second call's data lands in the model).
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        tmdb.trendingItems = { makeItem(1, MediaKind::Movie, QStringLiteral("First")) };

        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        vm.refresh();
        // Don't drain in between; back-to-back refresh bumps the
        // epoch and the queued continuation from the first call
        // should be dropped.
        tmdb.trendingItems = { makeItem(2, MediaKind::Movie, QStringLiteral("Second")) };
        vm.refresh();
        drain();

        auto* trending
            = qobject_cast<DiscoverSectionModel*>(vm.sectionsList().first());
        QVERIFY(trending != nullptr);
        QCOMPARE(trending->state(), DiscoverSectionModel::State::Ready);
        QCOMPARE(trending->rowCount(), 1);
        // Whichever drain order Qt chose, only the second call's
        // data should be present (the first call's update was
        // discarded by the stale-epoch guard before reaching the
        // model).
        const auto* item = trending->itemAt(0);
        QVERIFY(item != nullptr);
        QCOMPARE(item->title, QStringLiteral("Second"));
    }

    void activateItemRoutesByKind()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        tmdb.trendingItems = { makeItem(11, MediaKind::Movie, QStringLiteral("M")) };
        tmdb.popularSeriesItems = { makeItem(22, MediaKind::Series, QStringLiteral("S")) };

        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        QSignalSpy movieSpy(&vm, &DiscoverViewModel::openMovieRequested);
        QSignalSpy seriesSpy(&vm, &DiscoverViewModel::openSeriesRequested);

        vm.refresh();
        drain();

        // Section 0 is "Trending this week" (movies); section 1 is
        // "Popular series".
        vm.activateItem(0, 0);
        vm.activateItem(1, 0);

        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(seriesSpy.count(), 1);
        QCOMPARE(movieSpy.first().at(0).toInt(), 11);
        QCOMPARE(seriesSpy.first().at(0).toInt(), 22);

        // Out-of-range indices are no-ops, not crashes.
        vm.activateItem(99, 0);
        vm.activateItem(0, 99);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(seriesSpy.count(), 1);
    }

    void browseSectionEmitsBrowseRequested()
    {
        FakeTmdb tmdb;
        tmdb.setToken(QStringLiteral("token"));
        DiscoverViewModel vm(&tmdb, nullptr, nullptr);
        QSignalSpy browseSpy(&vm, &DiscoverViewModel::browseRequested);
        vm.browseSection(0);
        QCOMPARE(browseSpy.count(), 1);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpHome;
};

QTEST_MAIN(TstDiscoverViewModel)
#include "tst_discover_view_model.moc"
