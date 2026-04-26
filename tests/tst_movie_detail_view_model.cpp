// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppSettings.h"
#include "config/TorrentioSettings.h"
#include "core/HttpError.h"
#include "services/StreamActions.h"
#include "TestDoubles.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/MovieDetailViewModel.h"
#include "ui/qml-bridge/StreamsListModel.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using kinema::api::DiscoverItem;
using kinema::api::MediaKind;
using kinema::api::MetaDetail;
using kinema::api::MetaSummary;
using kinema::api::Stream;
using kinema::config::AppSettings;
using kinema::core::HttpError;
using kinema::services::StreamActions;
using kinema::tests::FakeCinemetaClient;
using kinema::tests::FakeTmdbClient;
using kinema::tests::FakeTorrentioClient;
using kinema::tests::drainEvents;
using kinema::ui::qml::MovieDetailViewModel;
using kinema::ui::qml::StreamsListModel;

namespace {

MetaSummary makeSummary(const QString& imdb, const QString& title,
    int year = 2010)
{
    MetaSummary s;
    s.imdbId = imdb;
    s.kind = MediaKind::Movie;
    s.title = title;
    s.year = year;
    return s;
}

MetaDetail makeDetail(const QString& imdb, const QString& title,
    std::optional<QDate> released = std::nullopt)
{
    MetaDetail d;
    d.summary = makeSummary(imdb, title);
    d.summary.description = QStringLiteral("synopsis");
    d.summary.released = released;
    d.summary.poster = QUrl(QStringLiteral("https://img/poster.jpg"));
    d.background = QUrl(QStringLiteral("https://img/back.jpg"));
    d.genres = { QStringLiteral("Sci-Fi"), QStringLiteral("Action") };
    d.runtimeMinutes = 148;
    d.summary.imdbRating = 8.4;
    return d;
}

Stream makeStream(const QString& release, const QString& resolution,
    int seeders, qint64 sizeBytes,
    const QString& provider = QStringLiteral("p"),
    bool hasDirectUrl = false)
{
    Stream s;
    s.releaseName = release;
    s.resolution = resolution;
    s.provider = provider;
    s.seeders = seeders;
    s.sizeBytes = sizeBytes;
    s.infoHash = QStringLiteral("0123456789abcdef");
    if (hasDirectUrl) {
        s.directUrl = QUrl(QStringLiteral(
            "https://rd.local/stream"));
    }
    return s;
}

struct Fixture {
    QTemporaryDir tmp;
    KSharedConfigPtr config;
    AppSettings settings;
    FakeCinemetaClient cinemeta;
    FakeTorrentioClient torrentio;
    FakeTmdbClient tmdb;
    StreamActions actions { nullptr };
    QString rdToken;
    MovieDetailViewModel vm;

    Fixture()
        : config(KSharedConfig::openConfig(
            tmp.filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig))
        , settings(config)
        , vm(&cinemeta, &torrentio, &tmdb, &actions,
              /*tokens=*/nullptr, settings, rdToken, nullptr)
    {
    }
};

} // namespace

class TstMovieDetailViewModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testInitialState()
    {
        Fixture f;
        QCOMPARE(f.vm.metaState(), MovieDetailViewModel::MetaState::Idle);
        QVERIFY(f.vm.title().isEmpty());
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Idle);
        QVERIFY(f.vm.similar() != nullptr);
        QVERIFY(!f.vm.similarVisible());
    }

    void testLoadFetchesMetaThenStreams()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt0133093"),
                QStringLiteral("The Matrix")) }
        };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("Matrix.1080p"),
                  QStringLiteral("1080p"), 50, 2'000'000'000) } }
        };

        f.vm.load(QStringLiteral("tt0133093"));
        drainEvents();

        QCOMPARE(f.vm.metaState(),
            MovieDetailViewModel::MetaState::Ready);
        QCOMPARE(f.vm.title(), QStringLiteral("The Matrix"));
        QCOMPARE(f.vm.year(), 2010);
        QCOMPARE(f.vm.runtimeMinutes(), 148);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Ready);
        QCOMPARE(f.vm.streams()->rowCount(), 1);
    }

    void testFutureReleaseSkipsTorrentioFetch()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt00001"),
                QStringLiteral("Soon"),
                QDate::currentDate().addDays(30)) }
        };

        f.vm.load(QStringLiteral("tt00001"));
        drainEvents();

        QCOMPARE(f.torrentio.callCount, 0);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Unreleased);
        QVERIFY(f.vm.isUpcoming());
    }

    void testStreamFetchErrorShowsError()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt1"),
                QStringLiteral("Matrix")) }
        };
        f.torrentio.scriptedCalls = {
            { {}, HttpError(HttpError::Kind::Network, 0,
                  QStringLiteral("torrentio down")) }
        };

        f.vm.load(QStringLiteral("tt1"));
        drainEvents();

        QCOMPARE(f.vm.metaState(),
            MovieDetailViewModel::MetaState::Ready);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Error);
        QVERIFY(!f.vm.streams()->errorMessage().isEmpty());
    }

    void testMetaErrorShowsErrorAndSkipsStreams()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { {}, HttpError(HttpError::Kind::HttpStatus, 404,
                  QStringLiteral("not found")) }
        };

        f.vm.load(QStringLiteral("tt-bogus"));
        drainEvents();

        QCOMPARE(f.vm.metaState(),
            MovieDetailViewModel::MetaState::Error);
        QVERIFY(!f.vm.metaError().isEmpty());
        QCOMPARE(f.torrentio.callCount, 0);
    }

    void testStaleResponseDiscarded()
    {
        Fixture f;
        // Two loads in quick succession; only the second should
        // populate the model, regardless of fake-resolution order.
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt-stale"),
                QStringLiteral("Stale")), std::nullopt, true },
            { makeDetail(QStringLiteral("tt-fresh"),
                QStringLiteral("Fresh")) }
        };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("Fresh.1080p"),
                  QStringLiteral("1080p"), 5, 1) } }
        };

        f.vm.load(QStringLiteral("tt-stale"));
        f.vm.load(QStringLiteral("tt-fresh"));
        drainEvents(4);

        QCOMPARE(f.vm.imdbId(), QStringLiteral("tt-fresh"));
        QCOMPARE(f.vm.title(), QStringLiteral("Fresh"));
        QCOMPARE(f.vm.streams()->rowCount(), 1);
    }

    void testRetryReusesCurrentImdbId()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt1"),
                QStringLiteral("First")) },
            { makeDetail(QStringLiteral("tt1"),
                QStringLiteral("First v2")) }
        };
        f.torrentio.scriptedCalls = {
            { {} },
            { {} },
        };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();
        QCOMPARE(f.cinemeta.metaCalls, 1);

        f.vm.retry();
        drainEvents();
        QCOMPARE(f.cinemeta.metaCalls, 2);
    }

    void testSortOrdersStreams()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt1"),
                QStringLiteral("X")) }
        };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("Smol"),
                    QStringLiteral("720p"), 1, 100),
                makeStream(QStringLiteral("Big"),
                    QStringLiteral("1080p"), 99, 200),
                makeStream(QStringLiteral("Mid"),
                    QStringLiteral("1080p"), 50, 150) } }
        };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();

        // Default sort: Seeders desc \u2192 Big, Mid, Smol.
        QCOMPARE(f.vm.streams()->rowCount(), 3);
        QCOMPARE(f.vm.streams()->at(0)->releaseName,
            QStringLiteral("Big"));

        // Switch to size asc.
        f.vm.setSortMode(static_cast<int>(
            StreamsListModel::SortMode::Size));
        f.vm.setSortDescending(false);
        QCOMPARE(f.vm.streams()->at(0)->releaseName,
            QStringLiteral("Smol"));
    }

    void testCachedOnlyFiltersWhenRdConfigured()
    {
        Fixture f;
        f.rdToken = QStringLiteral("rd"); // pretend RD is on
        f.settings.torrentio().setCachedOnly(true);

        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt1"),
                QStringLiteral("X")) }
        };
        Stream cached = makeStream(QStringLiteral("Cached.1080p"),
            QStringLiteral("1080p"), 5, 1);
        cached.rdCached = true;
        const Stream uncached = makeStream(QStringLiteral("Uncached"),
            QStringLiteral("1080p"), 5, 1);
        f.torrentio.scriptedCalls = { { { uncached, cached } } };

        f.vm.load(QStringLiteral("tt1"));
        drainEvents();

        QCOMPARE(f.vm.streams()->rowCount(), 1);
        QCOMPARE(f.vm.streams()->at(0)->releaseName,
            QStringLiteral("Cached.1080p"));
    }

    void testPlayBestEmitsStatusOnEmpty()
    {
        Fixture f;
        QSignalSpy spy(&f.vm,
            &MovieDetailViewModel::statusMessage);
        f.vm.playBest();
        QCOMPARE(spy.count(), 1);
    }

    void testPlayWithoutDirectUrlEmitsStatus()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt1"),
                QStringLiteral("X")) }
        };
        // No direct URL on the stream \u2192 play() should bail with
        // a status message instead of crashing into PlayerLauncher.
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("R"),
                  QStringLiteral("1080p"), 5, 1) } }
        };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();

        QSignalSpy spy(&f.vm,
            &MovieDetailViewModel::statusMessage);
        f.vm.play(0);
        QCOMPARE(spy.count(), 1);
    }

    void testActivateSimilarRoutesByKind()
    {
        Fixture f;
        DiscoverItem movieItem;
        movieItem.tmdbId = 27205;
        movieItem.kind = MediaKind::Movie;
        movieItem.title = QStringLiteral("Inception");
        DiscoverItem seriesItem;
        seriesItem.tmdbId = 1399;
        seriesItem.kind = MediaKind::Series;
        seriesItem.title = QStringLiteral("GoT");
        f.vm.similar()->setItems({ movieItem, seriesItem });

        QSignalSpy movieSpy(&f.vm,
            &MovieDetailViewModel::openMovieByTmdbRequested);
        QSignalSpy seriesSpy(&f.vm,
            &MovieDetailViewModel::openSeriesByTmdbRequested);

        f.vm.activateSimilar(0);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(movieSpy.first().at(0).toInt(), 27205);

        f.vm.activateSimilar(1);
        QCOMPARE(seriesSpy.count(), 1);
        QCOMPARE(seriesSpy.first().at(0).toInt(), 1399);

        // Out-of-range row is a silent no-op.
        f.vm.activateSimilar(99);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(seriesSpy.count(), 1);
    }

    void testLoadByTmdbResolvesAndLoads()
    {
        Fixture f;
        f.tmdb.movieImdbId = QStringLiteral("tt1375666");
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt1375666"),
                QStringLiteral("Inception")) }
        };
        f.torrentio.scriptedCalls = { { {} } };

        f.vm.loadByTmdbId(27205, QStringLiteral("Inception"));
        drainEvents();

        QCOMPARE(f.tmdb.movieLookupCalls, 1);
        QCOMPARE(f.vm.imdbId(), QStringLiteral("tt1375666"));
    }

    void testLoadByTmdb404SurfacesStatus()
    {
        Fixture f;
        f.tmdb.movieLookupError = HttpError(
            HttpError::Kind::HttpStatus, 404,
            QStringLiteral("nope"));

        QSignalSpy spy(&f.vm,
            &MovieDetailViewModel::statusMessage);
        f.vm.loadByTmdbId(99'999, QStringLiteral("Phantom"));
        drainEvents();

        // At least the "Looking up" + "isn't reachable" messages.
        QVERIFY(spy.count() >= 2);
        // Cinemeta should never have been hit: the resolve step failed.
        QCOMPARE(f.cinemeta.metaCalls, 0);
    }

    void testClearResetsState()
    {
        Fixture f;
        f.cinemeta.metaScripts = {
            { makeDetail(QStringLiteral("tt1"),
                QStringLiteral("X")) }
        };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("R"),
                  QStringLiteral("1080p"), 5, 1) } }
        };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();
        QVERIFY(!f.vm.title().isEmpty());

        f.vm.clear();
        QCOMPARE(f.vm.metaState(),
            MovieDetailViewModel::MetaState::Idle);
        QVERIFY(f.vm.title().isEmpty());
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Idle);
        QVERIFY(!f.vm.similarVisible());
    }
};

QTEST_MAIN(TstMovieDetailViewModel)
#include "tst_movie_detail_view_model.moc"
