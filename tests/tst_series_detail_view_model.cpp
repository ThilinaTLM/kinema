// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppSettings.h"
#include "core/HttpError.h"
#include "services/StreamActions.h"
#include "TestDoubles.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/EpisodesListModel.h"
#include "ui/qml-bridge/SeriesDetailViewModel.h"
#include "ui/qml-bridge/StreamsListModel.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using kinema::api::Episode;
using kinema::api::MediaKind;
using kinema::api::MetaDetail;
using kinema::api::SeriesDetail;
using kinema::api::Stream;
using kinema::config::AppSettings;
using kinema::core::HttpError;
using kinema::services::StreamActions;
using kinema::tests::FakeCinemetaClient;
using kinema::tests::FakeTmdbClient;
using kinema::tests::FakeTorrentioClient;
using kinema::tests::drainEvents;
using kinema::ui::qml::SeriesDetailViewModel;
using kinema::ui::qml::StreamsListModel;

namespace {

Episode makeEp(int season, int number, const QString& title,
    std::optional<QDate> released = std::nullopt)
{
    Episode e;
    e.season = season;
    e.number = number;
    e.title = title;
    e.description = QStringLiteral("desc");
    e.released = released;
    return e;
}

SeriesDetail makeSeries(const QString& imdb, const QString& title,
    QList<Episode> episodes)
{
    SeriesDetail sd;
    sd.meta.summary.imdbId = imdb;
    sd.meta.summary.kind = MediaKind::Series;
    sd.meta.summary.title = title;
    sd.meta.summary.year = 2008;
    sd.meta.summary.description = QStringLiteral("synopsis");
    sd.meta.summary.poster = QUrl(QStringLiteral("https://p"));
    sd.meta.background = QUrl(QStringLiteral("https://b"));
    sd.meta.genres = { QStringLiteral("Drama") };
    sd.episodes = std::move(episodes);
    return sd;
}

Stream makeStream(const QString& release, int seeders, qint64 size,
    bool hasDirectUrl = false)
{
    Stream s;
    s.releaseName = release;
    s.resolution = QStringLiteral("1080p");
    s.provider = QStringLiteral("p");
    s.seeders = seeders;
    s.sizeBytes = size;
    s.infoHash = QStringLiteral("0123456789abcdef");
    if (hasDirectUrl) {
        s.directUrl = QUrl(QStringLiteral("https://rd/x"));
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
    SeriesDetailViewModel vm;

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

class TstSeriesDetailViewModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testInitialState()
    {
        Fixture f;
        QCOMPARE(f.vm.metaState(),
            SeriesDetailViewModel::MetaState::Idle);
        QVERIFY(f.vm.title().isEmpty());
        QCOMPARE(f.vm.seasonLabels().size(), 0);
        QCOMPARE(f.vm.currentSeason(), -1);
        QCOMPARE(f.vm.selectedEpisodeRow(), -1);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Idle);
    }

    void testLoadPopulatesMetaAndSeasons()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt0903747"),
            QStringLiteral("Breaking Bad"),
            { makeEp(1, 1, QStringLiteral("Pilot")),
              makeEp(1, 2, QStringLiteral("Cat's in the Bag")),
              makeEp(2, 1, QStringLiteral("Seven Thirty-Seven")),
              makeEp(0, 1, QStringLiteral("Special")) }) } };

        f.vm.load(QStringLiteral("tt0903747"));
        drainEvents();

        QCOMPARE(f.vm.metaState(),
            SeriesDetailViewModel::MetaState::Ready);
        QCOMPARE(f.vm.title(), QStringLiteral("Breaking Bad"));
        // Specials excluded \u2192 two seasons.
        QCOMPARE(f.vm.seasonLabels().size(), 2);
        QCOMPARE(f.vm.currentSeason(), 0);
        QCOMPARE(f.vm.episodes()->rowCount(), 2);
    }

    void testSeasonSwitchPublishesNewEpisodes()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt1"), QStringLiteral("X"),
            { makeEp(1, 1, QStringLiteral("S1E1")),
              makeEp(2, 1, QStringLiteral("S2E1")),
              makeEp(2, 2, QStringLiteral("S2E2")) }) } };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();

        QCOMPARE(f.vm.episodes()->rowCount(), 1);
        f.vm.setCurrentSeason(1);
        QCOMPARE(f.vm.episodes()->rowCount(), 2);
        // Switching seasons collapses the streams region.
        QCOMPARE(f.vm.selectedEpisodeRow(), -1);
    }

    void testSelectEpisodeFetchesStreams()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt1"), QStringLiteral("X"),
            { makeEp(1, 1, QStringLiteral("Pilot")) }) } };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("Pilot.1080p"),
                  10, 1'000'000'000) } }
        };

        f.vm.load(QStringLiteral("tt1"));
        drainEvents();

        f.vm.selectEpisode(0);
        drainEvents();

        QCOMPARE(f.vm.selectedEpisodeRow(), 0);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Ready);
        QCOMPARE(f.vm.streams()->rowCount(), 1);
        QCOMPARE(f.torrentio.lastStreamId,
            QStringLiteral("tt1:1:1"));
    }

    void testFutureEpisodeSkipsTorrentioFetch()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt1"), QStringLiteral("X"),
            { makeEp(1, 1, QStringLiteral("Future"),
                  QDate::currentDate().addDays(30)) }) } };

        f.vm.load(QStringLiteral("tt1"));
        drainEvents();

        f.vm.selectEpisode(0);
        drainEvents();

        QCOMPARE(f.torrentio.callCount, 0);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Unreleased);
    }

    void testEpisodeStreamsErrorShowsError()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt1"), QStringLiteral("X"),
            { makeEp(1, 1, QStringLiteral("Pilot")) }) } };
        f.torrentio.scriptedCalls = {
            { {}, HttpError(HttpError::Kind::Network, 0,
                  QStringLiteral("down")) }
        };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();
        f.vm.selectEpisode(0);
        drainEvents();

        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Error);
    }

    void testClearEpisodeCollapsesStreams()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt1"), QStringLiteral("X"),
            { makeEp(1, 1, QStringLiteral("Pilot")) }) } };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("R"), 1, 1) } }
        };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();
        f.vm.selectEpisode(0);
        drainEvents();
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Ready);

        f.vm.clearEpisode();
        QCOMPARE(f.vm.selectedEpisodeRow(), -1);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Idle);
    }

    void testLoadAtAutoSelectsEpisode()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt1"), QStringLiteral("X"),
            { makeEp(1, 1, QStringLiteral("S1E1")),
              makeEp(2, 1, QStringLiteral("S2E1")),
              makeEp(2, 2, QStringLiteral("S2E2")) }) } };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("S2E2"), 1, 1) } }
        };

        f.vm.loadAt(QStringLiteral("tt1"), 2, 2);
        drainEvents();

        QCOMPARE(f.vm.currentSeason(), 1); // index of season 2
        QCOMPARE(f.vm.selectedEpisodeRow(), 1);
        QCOMPARE(f.torrentio.lastStreamId,
            QStringLiteral("tt1:2:2"));
    }

    void testStaleResponseDiscarded()
    {
        Fixture f;
        f.cinemeta.seriesScripts = {
            { makeSeries(QStringLiteral("tt-stale"),
                  QStringLiteral("Stale"),
                  { makeEp(1, 1, QStringLiteral("S")) }),
                std::nullopt, true },
            { makeSeries(QStringLiteral("tt-fresh"),
                  QStringLiteral("Fresh"),
                  { makeEp(1, 1, QStringLiteral("F")) }) }
        };

        f.vm.load(QStringLiteral("tt-stale"));
        f.vm.load(QStringLiteral("tt-fresh"));
        drainEvents(4);

        QCOMPARE(f.vm.imdbId(), QStringLiteral("tt-fresh"));
        QCOMPARE(f.vm.title(), QStringLiteral("Fresh"));
    }

    void testMetaErrorShowsError()
    {
        Fixture f;
        f.cinemeta.seriesScripts = {
            { {}, HttpError(HttpError::Kind::HttpStatus, 500,
                  QStringLiteral("oops")) }
        };
        f.vm.load(QStringLiteral("tt-bogus"));
        drainEvents();

        QCOMPARE(f.vm.metaState(),
            SeriesDetailViewModel::MetaState::Error);
        QVERIFY(!f.vm.metaError().isEmpty());
    }

    void testLoadByTmdbResolvesAndLoads()
    {
        Fixture f;
        f.tmdb.seriesImdbId = QStringLiteral("tt0903747");
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt0903747"),
            QStringLiteral("Breaking Bad"),
            { makeEp(1, 1, QStringLiteral("Pilot")) }) } };

        f.vm.loadByTmdbId(1396, QStringLiteral("Breaking Bad"));
        drainEvents();

        QCOMPARE(f.tmdb.seriesLookupCalls, 1);
        QCOMPARE(f.vm.imdbId(), QStringLiteral("tt0903747"));
    }

    void testActivateSimilarRoutesByKind()
    {
        Fixture f;
        kinema::api::DiscoverItem movie;
        movie.tmdbId = 100;
        movie.kind = MediaKind::Movie;
        movie.title = QStringLiteral("M");
        kinema::api::DiscoverItem series;
        series.tmdbId = 200;
        series.kind = MediaKind::Series;
        series.title = QStringLiteral("S");
        f.vm.similar()->setItems({ movie, series });

        QSignalSpy movieSpy(&f.vm,
            &SeriesDetailViewModel::openMovieByTmdbRequested);
        QSignalSpy seriesSpy(&f.vm,
            &SeriesDetailViewModel::openSeriesByTmdbRequested);

        f.vm.activateSimilar(0);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(movieSpy.first().at(0).toInt(), 100);

        f.vm.activateSimilar(1);
        QCOMPARE(seriesSpy.count(), 1);
        QCOMPARE(seriesSpy.first().at(0).toInt(), 200);
    }

    void testClearResetsAll()
    {
        Fixture f;
        f.cinemeta.seriesScripts = { { makeSeries(
            QStringLiteral("tt1"), QStringLiteral("X"),
            { makeEp(1, 1, QStringLiteral("Pilot")) }) } };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("R"), 1, 1) } }
        };
        f.vm.load(QStringLiteral("tt1"));
        drainEvents();
        f.vm.selectEpisode(0);
        drainEvents();
        QVERIFY(!f.vm.title().isEmpty());

        f.vm.clear();
        QCOMPARE(f.vm.metaState(),
            SeriesDetailViewModel::MetaState::Idle);
        QVERIFY(f.vm.title().isEmpty());
        QCOMPARE(f.vm.seasonLabels().size(), 0);
        QCOMPARE(f.vm.currentSeason(), -1);
        QCOMPARE(f.vm.selectedEpisodeRow(), -1);
        QCOMPARE(f.vm.streams()->state(),
            StreamsListModel::State::Idle);
    }
};

QTEST_MAIN(TstSeriesDetailViewModel)
#include "tst_series_detail_view_model.moc"
