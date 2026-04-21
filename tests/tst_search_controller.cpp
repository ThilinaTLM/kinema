// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/CinemetaClient.h"
#include "api/Media.h"
#include "controllers/NavigationController.h"
#include "controllers/Page.h"
#include "controllers/SearchController.h"
#include "core/HttpError.h"
#include "ui/ResultsModel.h"
#include "ui/StateWidget.h"

#include <QApplication>
#include <QSignalSpy>
#include <QStackedWidget>
#include <QTest>
#include <QWidget>

using kinema::api::CinemetaClient;
using kinema::api::MediaKind;
using kinema::api::MetaDetail;
using kinema::api::MetaSummary;
using kinema::controllers::NavigationController;
using kinema::controllers::Page;
using kinema::controllers::SearchController;
using kinema::ui::ResultsModel;
using kinema::ui::StateWidget;

namespace {

/// In-memory CinemetaClient stand-in. Override the three virtual
/// endpoints so SearchController's coroutines complete synchronously
/// (from Qt's event loop perspective) with canned data \u2014 no network,
/// no HTTP server.
class FakeCinemeta : public CinemetaClient
{
public:
    FakeCinemeta()
        : CinemetaClient(nullptr)
    {
    }

    QList<MetaSummary> cannedSearch;
    MetaDetail cannedMeta;
    bool throwHttpError = false;
    int searchCalls = 0;
    int metaCalls = 0;
    QString lastQuery;

    QCoro::Task<QList<MetaSummary>> search(MediaKind /*kind*/,
        QString query) override
    {
        ++searchCalls;
        lastQuery = query;
        if (throwHttpError) {
            throw kinema::core::HttpError(
                kinema::core::HttpError::Kind::Network, 0,
                QStringLiteral("canned failure"));
        }
        co_return cannedSearch;
    }

    QCoro::Task<MetaDetail> meta(MediaKind /*kind*/,
        QString /*imdbId*/) override
    {
        ++metaCalls;
        if (throwHttpError) {
            throw kinema::core::HttpError(
                kinema::core::HttpError::Kind::Network, 0,
                QStringLiteral("canned failure"));
        }
        co_return cannedMeta;
    }
};

/// Shared fixture: a minimal widget tree + nav + SearchController
/// wired the same way MainWindow wires them.
struct Fixture {
    QStackedWidget centerStack;
    QStackedWidget resultsStack;
    QWidget discover;
    QWidget browse;
    StateWidget searchState;
    QWidget searchResults;
    QWidget movieDetail;
    QWidget seriesDetail;

    NavigationController nav {
        &centerStack, &resultsStack,
        &discover, &browse, &searchState, &searchResults,
        &movieDetail, &seriesDetail,
        nullptr
    };

    FakeCinemeta cinemeta;
    ResultsModel model;
    SearchController controller {
        &cinemeta, &model, &searchState, &nav, nullptr
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
    }

    /// Let Qt run any queued coroutine continuations.
    static void drain()
    {
        QCoreApplication::processEvents();
        QCoreApplication::processEvents();
    }
};

} // namespace

class TstSearchController : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testSuccessfulSearchFlipsToResults()
    {
        Fixture f;
        MetaSummary s;
        s.imdbId = QStringLiteral("tt0111161");
        s.title = QStringLiteral("The Shawshank Redemption");
        s.kind = MediaKind::Movie;
        f.cinemeta.cannedSearch = { s };

        QSignalSpy readySpy(&f.controller,
            &SearchController::resultsReady);

        f.controller.runQuery(
            QStringLiteral("shawshank"), MediaKind::Movie);
        Fixture::drain();

        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SearchResults));
        QCOMPARE(f.model.rowCount(), 1);
        QCOMPARE(readySpy.count(), 1);
        QCOMPARE(readySpy.first().at(0).toInt(), 1);
        QCOMPARE(f.cinemeta.searchCalls, 1);
        QCOMPARE(f.cinemeta.lastQuery, QStringLiteral("shawshank"));
    }

    void testEmptyResultsFlipsToSearchState()
    {
        Fixture f;
        f.cinemeta.cannedSearch = {};

        f.controller.runQuery(QStringLiteral("xyznomatches"),
            MediaKind::Movie);
        Fixture::drain();

        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SearchState));
        QCOMPARE(f.model.rowCount(), 0);
    }

    void testImdbIdShortcutUsesMeta()
    {
        Fixture f;
        MetaDetail d;
        d.summary.imdbId = QStringLiteral("tt0111161");
        d.summary.title = QStringLiteral("Shawshank");
        d.summary.kind = MediaKind::Movie;
        f.cinemeta.cannedMeta = d;

        f.controller.runQuery(
            QStringLiteral("tt0111161"), MediaKind::Movie);
        Fixture::drain();

        QCOMPARE(f.cinemeta.metaCalls, 1);
        QCOMPARE(f.cinemeta.searchCalls, 0);
        QCOMPARE(f.model.rowCount(), 1);
        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SearchResults));
    }

    void testNonImdbQueryUsesSearch()
    {
        Fixture f;
        MetaSummary s;
        s.imdbId = QStringLiteral("tt0111161");
        s.title = QStringLiteral("x");
        f.cinemeta.cannedSearch = { s };

        f.controller.runQuery(QStringLiteral("tt"), MediaKind::Movie);
        Fixture::drain();

        QCOMPARE(f.cinemeta.metaCalls, 0);
        QCOMPARE(f.cinemeta.searchCalls, 1);
    }

    void testErrorPathFlipsToSearchState()
    {
        Fixture f;
        f.cinemeta.throwHttpError = true;

        QSignalSpy statusSpy(&f.controller,
            &SearchController::statusMessage);

        f.controller.runQuery(QStringLiteral("x"), MediaKind::Movie);
        Fixture::drain();

        QCOMPARE(static_cast<int>(f.nav.current()),
            static_cast<int>(Page::SearchState));
        // At least two status messages fire: the "Searching..." at
        // start and the "Search failed" on error.
        QVERIFY(statusSpy.count() >= 2);
    }

    void testQueryStartedSignalFires()
    {
        Fixture f;
        f.cinemeta.cannedSearch = {};

        QSignalSpy startSpy(&f.controller,
            &SearchController::queryStarted);
        f.controller.runQuery(QStringLiteral("x"), MediaKind::Movie);
        // queryStarted fires before the first co_await, i.e.
        // synchronously from runQuery(). No drain needed.
        QCOMPARE(startSpy.count(), 1);
    }
};

QTEST_MAIN(TstSearchController)
#include "tst_search_controller.moc"
