// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/MovieDetailController.h"

#include "config/AppSettings.h"
#include "controllers/NavigationController.h"
#include "controllers/Page.h"
#include "services/StreamActions.h"
#include "TestDoubles.h"
#include "ui/DetailPane.h"
#include "ui/TorrentsModel.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QStackedWidget>
#include <QTemporaryDir>
#include <QTest>
#include <QWidget>

using kinema::api::DiscoverItem;
using kinema::api::MediaKind;
using kinema::api::MetaDetail;
using kinema::api::MetaSummary;
using kinema::api::Stream;
using kinema::config::AppSettings;
using kinema::controllers::MovieDetailController;
using kinema::controllers::NavigationController;
using kinema::controllers::Page;
using kinema::core::HttpError;
using kinema::services::StreamActions;
using kinema::tests::FakeCinemetaClient;
using kinema::tests::FakeTmdbClient;
using kinema::tests::FakeTorrentioClient;
using kinema::tests::drainEvents;
using kinema::ui::DetailPane;

namespace {

MetaSummary makeMovieSummary(const QString& imdbId, const QString& title)
{
    MetaSummary s;
    s.imdbId = imdbId;
    s.kind = MediaKind::Movie;
    s.title = title;
    s.year = 1999;
    return s;
}

MetaDetail makeMovieDetail(const QString& imdbId, const QString& title)
{
    MetaDetail d;
    d.summary = makeMovieSummary(imdbId, title);
    d.summary.description = QStringLiteral("desc");
    d.genres = { QStringLiteral("Drama") };
    d.runtimeMinutes = 120;
    return d;
}

Stream makeStream(const QString& release)
{
    Stream s;
    s.releaseName = release;
    s.resolution = QStringLiteral("1080p");
    s.infoHash = QStringLiteral("abcdef0123456789");
    s.seeders = 50;
    return s;
}

struct Fixture {
    QTemporaryDir tmpDir;
    KSharedConfigPtr config;
    AppSettings settings;
    StreamActions actions { nullptr };

    QStackedWidget centerStack;
    QStackedWidget resultsStack;
    QWidget discover;
    QWidget browse;
    QWidget searchState;
    QWidget searchResults;
    QWidget seriesDetail;
    DetailPane movieDetail { nullptr, nullptr,
        settings.torrentio(), settings.filter(), actions,
        settings.appearance() };

    NavigationController nav {
        &centerStack, &resultsStack,
        &discover, &browse, &searchState, &searchResults,
        &movieDetail, &seriesDetail,
        nullptr
    };

    FakeCinemetaClient cinemeta;
    FakeTorrentioClient torrentio;
    FakeTmdbClient tmdb;
    QString rdToken { QStringLiteral("rd-token") };
    MovieDetailController controller {
        &cinemeta, &torrentio, &tmdb,
        &movieDetail, &nav, settings, rdToken,
        nullptr
    };

    Fixture()
        : config(KSharedConfig::openConfig(
            tmpDir.filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig))
        , settings(config)
    {
        resultsStack.addWidget(&discover);
        resultsStack.addWidget(&searchState);
        resultsStack.addWidget(&searchResults);
        resultsStack.addWidget(&browse);
        centerStack.addWidget(&resultsStack);
        centerStack.addWidget(&movieDetail);
        centerStack.addWidget(&seriesDetail);
    }
};

} // namespace

class TstMovieDetailController : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testOpenFromSummaryFetchesMetaAndStreams()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeMovieDetail(QStringLiteral("tt0133093"), QStringLiteral("The Matrix")) }
        };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("Matrix.1080p")) } }
        };

        const auto summary = makeMovieSummary(
            QStringLiteral("tt0133093"), QStringLiteral("The Matrix"));
        f.controller.openFromSummary(summary);
        drainEvents();

        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::MovieDetail));
        QVERIFY(f.controller.current().has_value());
        QCOMPARE(f.controller.current()->imdbId, QStringLiteral("tt0133093"));
        QCOMPARE(f.movieDetail.torrentsModel()->rowCount(), 1);
        QCOMPARE(f.torrentio.lastOptions.realDebridToken, QStringLiteral("rd-token"));
    }

    void testOpenFromDiscoverResolvesImdbAndHandsOff()
    {
        Fixture f;
        f.tmdb.movieImdbId = QStringLiteral("tt1375666");
        f.cinemeta.metaScripts = {
            { makeMovieDetail(QStringLiteral("tt1375666"), QStringLiteral("Inception")) }
        };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("Inception.1080p")) } }
        };

        DiscoverItem item;
        item.kind = MediaKind::Movie;
        item.tmdbId = 27205;
        item.title = QStringLiteral("Inception");
        item.year = 2010;
        item.overview = QStringLiteral("dreams");
        item.voteAverage = 8.3;

        QSignalSpy statusSpy(&f.controller, &MovieDetailController::statusMessage);
        f.controller.openFromDiscover(item);
        drainEvents();

        QCOMPARE(f.tmdb.movieLookupCalls, 1);
        QCOMPARE(f.cinemeta.lastMetaImdbId, QStringLiteral("tt1375666"));
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::MovieDetail));
        QVERIFY(statusSpy.count() >= 1);
        QCOMPARE(f.controller.current()->imdbId, QStringLiteral("tt1375666"));
    }

    void testFutureReleaseSkipsTorrentFetch()
    {
        Fixture f;
        auto unreleased = makeMovieDetail(
            QStringLiteral("tt0000001"), QStringLiteral("Future Movie"));
        unreleased.summary.released = QDate::currentDate().addDays(10);
        f.cinemeta.metaScripts = { { unreleased } };

        f.controller.openFromSummary(unreleased.summary);
        drainEvents();

        QCOMPARE(f.torrentio.callCount, 0);
        QCOMPARE(f.movieDetail.torrentsModel()->rowCount(), 0);
    }

    void testStreamFetchErrorDoesNotCrashAndLeavesNoRows()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeMovieDetail(QStringLiteral("tt0133093"), QStringLiteral("The Matrix")) }
        };
        f.torrentio.scriptedCalls = {
            { {}, HttpError(HttpError::Kind::Network, 0,
                  QStringLiteral("torrentio down")) }
        };

        f.controller.openFromSummary(makeMovieSummary(
            QStringLiteral("tt0133093"), QStringLiteral("The Matrix")));
        drainEvents();

        QCOMPARE(f.torrentio.callCount, 1);
        QCOMPARE(f.movieDetail.torrentsModel()->rowCount(), 0);
        QVERIFY(f.controller.current().has_value());
    }

    void testStaleResponseIsSuppressed()
    {
        Fixture f;
        auto stale = makeMovieDetail(QStringLiteral("tt0000001"), QStringLiteral("Old"));
        stale.summary.released = QDate::currentDate().addDays(7);
        auto current = makeMovieDetail(QStringLiteral("tt0000002"), QStringLiteral("New"));
        f.cinemeta.metaScripts = {
            { stale, std::nullopt, true },
            { current }
        };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("New.1080p")) } }
        };

        f.controller.openFromSummary(stale.summary);
        f.controller.openFromSummary(current.summary);
        drainEvents(4);

        QCOMPARE(f.torrentio.callCount, 1);
        QCOMPARE(f.movieDetail.torrentsModel()->rowCount(), 1);
        QCOMPARE(f.controller.current()->imdbId, QStringLiteral("tt0000002"));
    }
};

QTEST_MAIN(TstMovieDetailController)
#include "tst_movie_detail_controller.moc"
