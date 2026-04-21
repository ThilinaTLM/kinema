// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/NavigationController.h"
#include "controllers/Page.h"

#include <QApplication>
#include <QSignalSpy>
#include <QStackedWidget>
#include <QTest>
#include <QWidget>

using kinema::controllers::NavigationController;
using kinema::controllers::Page;

namespace {

/// Test fixture: assembles the same two-level QStackedWidget tree the
/// real MainWindow uses, with plain QWidget placeholders for every
/// surface. NavigationController only ever calls setCurrentWidget, so
/// no concrete pane type is required.
struct Fixture {
    QStackedWidget centerStack;
    QStackedWidget resultsStack;

    QWidget discover;
    QWidget browse;
    QWidget searchState;
    QWidget searchResults;
    QWidget movieDetail;
    QWidget seriesDetail;

    NavigationController nav {
        &centerStack, &resultsStack,
        &discover, &browse, &searchState, &searchResults,
        &movieDetail, &seriesDetail,
        nullptr
    };

    Fixture()
    {
        resultsStack.addWidget(&discover);
        resultsStack.addWidget(&searchState);
        resultsStack.addWidget(&searchResults);
        resultsStack.addWidget(&browse);

        centerStack.addWidget(&resultsStack);
        centerStack.addWidget(&movieDetail);
        centerStack.addWidget(&seriesDetail);

        // Start on Discover so canGoBack() starts false.
        resultsStack.setCurrentWidget(&discover);
        centerStack.setCurrentWidget(&resultsStack);
    }
};

} // namespace

class TstNavigation : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    // ---- Initial state ---------------------------------------------------

    void testInitialStateIsDiscoverHome()
    {
        Fixture f;
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::DiscoverHome));
        QVERIFY(!f.nav.canGoBack());
    }

    // ---- goTo applies widget transitions + emits --------------------------

    void testGoToSearchResults()
    {
        Fixture f;
        QSignalSpy spy(&f.nav, &NavigationController::currentChanged);

        f.nav.goTo(Page::SearchResults);
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SearchResults));
        QCOMPARE(spy.count(), 1);
        QCOMPARE(f.resultsStack.currentWidget(), &f.searchResults);
        QCOMPARE(f.centerStack.currentWidget(), &f.resultsStack);
        QVERIFY(f.nav.canGoBack());
    }

    void testGoToMovieDetailShowsDetailWidget()
    {
        Fixture f;
        f.nav.goTo(Page::SearchResults);
        f.nav.goTo(Page::MovieDetail);
        QCOMPARE(f.centerStack.currentWidget(), &f.movieDetail);
    }

    void testGoToSameIsNoOp()
    {
        Fixture f;
        QSignalSpy spy(&f.nav, &NavigationController::currentChanged);
        f.nav.goTo(Page::DiscoverHome);
        QCOMPARE(spy.count(), 0);
    }

    // ---- Back rules ------------------------------------------------------

    void testBackFromSearchResultsGoesHome()
    {
        Fixture f;
        f.nav.goTo(Page::SearchResults);
        f.nav.goBack();
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::DiscoverHome));
    }

    void testBackFromBrowseGoesHome()
    {
        Fixture f;
        f.nav.goTo(Page::Browse);
        f.nav.goBack();
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::DiscoverHome));
    }

    void testBackFromSeriesStreamsGoesToEpisodes()
    {
        Fixture f;
        f.nav.goTo(Page::SearchResults);
        f.nav.goTo(Page::SeriesEpisodes);
        f.nav.goTo(Page::SeriesStreams);
        f.nav.goBack();
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SeriesEpisodes));
    }

    void testBackFromDetailReturnsToResultsPageThatOpenedIt()
    {
        Fixture f;
        // Open detail from Browse: Back should return to Browse, not
        // Discover.
        f.nav.goTo(Page::Browse);
        f.nav.goTo(Page::MovieDetail);
        f.nav.goBack();
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::Browse));
    }

    void testBackFromDetailReturnsToSearchResultsWhenOpenedFromThere()
    {
        Fixture f;
        f.nav.goTo(Page::SearchResults);
        f.nav.goTo(Page::SeriesEpisodes);
        f.nav.goBack();
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SearchResults));
    }

    void testBackAtDiscoverHomeIsNoOp()
    {
        Fixture f;
        QSignalSpy spy(&f.nav, &NavigationController::currentChanged);
        f.nav.goBack();
        QCOMPARE(spy.count(), 0);
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::DiscoverHome));
    }

    // ---- canGoBack -------------------------------------------------------

    void testCanGoBackTracksCurrent()
    {
        Fixture f;
        QVERIFY(!f.nav.canGoBack());
        f.nav.goTo(Page::SearchState);
        QVERIFY(f.nav.canGoBack());
        f.nav.goTo(Page::DiscoverHome);
        QVERIFY(!f.nav.canGoBack());
    }

    // ---- Helpers ---------------------------------------------------------

    void testIsResultsPageHelpers()
    {
        using kinema::controllers::isDetailPage;
        using kinema::controllers::isResultsPage;

        QVERIFY(isResultsPage(Page::DiscoverHome));
        QVERIFY(isResultsPage(Page::SearchState));
        QVERIFY(isResultsPage(Page::SearchResults));
        QVERIFY(isResultsPage(Page::Browse));
        QVERIFY(!isResultsPage(Page::MovieDetail));
        QVERIFY(!isResultsPage(Page::SeriesEpisodes));
        QVERIFY(!isResultsPage(Page::SeriesStreams));

        QVERIFY(isDetailPage(Page::MovieDetail));
        QVERIFY(isDetailPage(Page::SeriesEpisodes));
        QVERIFY(isDetailPage(Page::SeriesStreams));
        QVERIFY(!isDetailPage(Page::DiscoverHome));
        QVERIFY(!isDetailPage(Page::Browse));
    }
};

QTEST_MAIN(TstNavigation)
#include "tst_navigation.moc"
