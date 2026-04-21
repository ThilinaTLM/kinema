// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/SeriesDetailController.h"

#include "config/AppSettings.h"
#include "controllers/NavigationController.h"
#include "controllers/Page.h"
#include "services/StreamActions.h"
#include "TestDoubles.h"
#include "ui/SeriesDetailPane.h"
#include "ui/TorrentsModel.h"

#include <KConfig>
#include <KSharedConfig>

#include <QStackedWidget>
#include <QTemporaryDir>
#include <QTest>
#include <QWidget>

using kinema::api::DiscoverItem;
using kinema::api::Episode;
using kinema::api::MediaKind;
using kinema::api::MetaDetail;
using kinema::api::MetaSummary;
using kinema::api::SeriesDetail;
using kinema::api::Stream;
using kinema::config::AppSettings;
using kinema::controllers::NavigationController;
using kinema::controllers::Page;
using kinema::controllers::SeriesDetailController;
using kinema::core::HttpError;
using kinema::services::StreamActions;
using kinema::tests::FakeCinemetaClient;
using kinema::tests::FakeTmdbClient;
using kinema::tests::FakeTorrentioClient;
using kinema::tests::drainEvents;
using kinema::ui::SeriesDetailPane;
using kinema::ui::TorrentsModel;

namespace {

MetaDetail makeSeriesMeta(const QString& imdbId, const QString& title)
{
    MetaDetail d;
    d.summary.imdbId = imdbId;
    d.summary.kind = MediaKind::Series;
    d.summary.title = title;
    d.summary.year = 2008;
    d.summary.description = QStringLiteral("desc");
    d.genres = { QStringLiteral("Drama") };
    return d;
}

Episode makeEpisode(int season, int number, const QString& title)
{
    Episode ep;
    ep.season = season;
    ep.number = number;
    ep.title = title;
    ep.released = QDate::currentDate().addDays(-1);
    return ep;
}

SeriesDetail makeSeriesDetail(const QString& imdbId, const QString& title)
{
    SeriesDetail d;
    d.meta = makeSeriesMeta(imdbId, title);
    d.episodes = {
        makeEpisode(1, 1, QStringLiteral("Pilot")),
        makeEpisode(1, 2, QStringLiteral("Cat's in the Bag"))
    };
    return d;
}

Stream makeStream(const QString& release)
{
    Stream s;
    s.releaseName = release;
    s.resolution = QStringLiteral("1080p");
    s.infoHash = QStringLiteral("abcdef0123456789");
    s.seeders = 42;
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
    QWidget movieDetail;
    SeriesDetailPane seriesDetail { nullptr, nullptr,
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
    SeriesDetailController controller {
        &cinemeta, &torrentio, &tmdb,
        &seriesDetail, &nav, settings, rdToken,
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

    TorrentsModel* torrentsModel()
    {
        return seriesDetail.findChild<TorrentsModel*>();
    }
};

} // namespace

class TstSeriesDetailController : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testOpenFromSummaryFetchesSeriesMeta()
    {
        Fixture f;
        const auto detail = makeSeriesDetail(
            QStringLiteral("tt0903747"), QStringLiteral("Breaking Bad"));
        f.cinemeta.seriesScripts = { { detail } };

        MetaSummary summary = detail.meta.summary;
        f.controller.openFromSummary(summary);
        drainEvents();

        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SeriesEpisodes));
        QCOMPARE(f.controller.currentImdbId(), QStringLiteral("tt0903747"));
        QVERIFY(!f.controller.currentEpisode().has_value());
    }

    void testOpenFromDiscoverResolvesImdbAndHandsOff()
    {
        Fixture f;
        f.tmdb.seriesImdbId = QStringLiteral("tt0944947");
        f.cinemeta.seriesScripts = {
            { makeSeriesDetail(QStringLiteral("tt0944947"), QStringLiteral("Game of Thrones")) }
        };

        DiscoverItem item;
        item.kind = MediaKind::Series;
        item.tmdbId = 1399;
        item.title = QStringLiteral("Game of Thrones");
        item.year = 2011;
        item.overview = QStringLiteral("winter");

        f.controller.openFromDiscover(item);
        drainEvents();

        QCOMPARE(f.tmdb.seriesLookupCalls, 1);
        QCOMPARE(f.controller.currentImdbId(), QStringLiteral("tt0944947"));
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SeriesEpisodes));
    }

    void testUnreleasedEpisodeSkipsTorrentFetch()
    {
        Fixture f;
        auto detail = makeSeriesDetail(
            QStringLiteral("tt0903747"), QStringLiteral("Breaking Bad"));
        f.cinemeta.seriesScripts = { { detail } };
        f.controller.openFromSummary(detail.meta.summary);
        drainEvents();

        Episode future = detail.episodes.first();
        future.released = QDate::currentDate().addDays(5);
        f.controller.selectEpisode(future);
        drainEvents();

        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SeriesStreams));
        QCOMPARE(f.torrentio.callCount, 0);
        QVERIFY(f.controller.currentEpisode().has_value());
    }

    void testBackToEpisodesAllowsReselection()
    {
        Fixture f;
        const auto detail = makeSeriesDetail(
            QStringLiteral("tt0903747"), QStringLiteral("Breaking Bad"));
        f.cinemeta.seriesScripts = { { detail } };
        f.torrentio.scriptedCalls = {
            { { makeStream(QStringLiteral("Pilot.1080p")) } },
            { { makeStream(QStringLiteral("Ep2.1080p")) } }
        };

        f.controller.openFromSummary(detail.meta.summary);
        drainEvents();
        f.controller.selectEpisode(detail.episodes.at(0));
        drainEvents();
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SeriesStreams));

        f.controller.onBackToEpisodes();
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SeriesEpisodes));
        QVERIFY(!f.controller.currentEpisode().has_value());

        f.controller.selectEpisode(detail.episodes.at(1));
        drainEvents();

        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SeriesStreams));
        QVERIFY(f.controller.currentEpisode().has_value());
        QCOMPARE(f.controller.currentEpisode()->number, 2);
        QVERIFY(f.torrentsModel());
        QCOMPARE(f.torrentsModel()->rowCount(), 1);
    }

    void testStaleEpisodeResponseIsSuppressed()
    {
        Fixture f;
        const auto detail = makeSeriesDetail(
            QStringLiteral("tt0903747"), QStringLiteral("Breaking Bad"));
        f.cinemeta.seriesScripts = { { detail } };
        f.torrentio.scriptedCalls = {
            { {}, HttpError(HttpError::Kind::Network, 0,
                  QStringLiteral("stale failure")), true },
            { { makeStream(QStringLiteral("Fresh.1080p")) } }
        };

        f.controller.openFromSummary(detail.meta.summary);
        drainEvents();

        f.controller.selectEpisode(detail.episodes.at(0));
        f.controller.selectEpisode(detail.episodes.at(1));
        drainEvents(4);

        QVERIFY(f.torrentsModel());
        QCOMPARE(f.torrentsModel()->rowCount(), 1);
        QVERIFY(f.controller.currentEpisode().has_value());
        QCOMPARE(f.controller.currentEpisode()->number, 2);
    }
};

QTEST_MAIN(TstSeriesDetailController)
#include "tst_series_detail_controller.moc"
