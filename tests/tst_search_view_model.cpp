// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/CinemetaClient.h"
#include "api/Media.h"
#include "core/HttpError.h"
#include "ui/qml-bridge/ResultsListModel.h"
#include "ui/qml-bridge/SearchViewModel.h"

#include <QSignalSpy>
#include <QTest>

using kinema::api::CinemetaClient;
using kinema::api::MediaKind;
using kinema::api::MetaDetail;
using kinema::api::MetaSummary;
using kinema::core::HttpError;
using kinema::ui::qml::ResultsListModel;
using kinema::ui::qml::SearchViewModel;

namespace {

/// In-memory CinemetaClient. Override the two virtual endpoints the
/// view-model uses (search / meta) and let the coroutines complete
/// synchronously from Qt's event loop. The fake intentionally
/// throws as a separate path because that's how the real client
/// surfaces 404s / network errors.
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
    MediaKind lastKind = MediaKind::Movie;

    QCoro::Task<QList<MetaSummary>> search(MediaKind kind,
        QString query) override
    {
        ++searchCalls;
        lastQuery = query;
        lastKind = kind;
        if (throwHttpError) {
            throw HttpError(HttpError::Kind::Network, 0,
                QStringLiteral("canned failure"));
        }
        co_return cannedSearch;
    }

    QCoro::Task<MetaDetail> meta(MediaKind /*kind*/,
        QString /*imdbId*/) override
    {
        ++metaCalls;
        if (throwHttpError) {
            throw HttpError(HttpError::Kind::Network, 0,
                QStringLiteral("canned failure"));
        }
        co_return cannedMeta;
    }
};

MetaSummary makeRow(const QString& imdb, const QString& title,
    MediaKind kind = MediaKind::Movie)
{
    MetaSummary s;
    s.imdbId = imdb;
    s.title = title;
    s.kind = kind;
    s.year = 2024;
    return s;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstSearchViewModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testInitialStateIdle()
    {
        FakeCinemeta cinemeta;
        SearchViewModel vm(&cinemeta, nullptr);
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Idle);
        QVERIFY(vm.query().isEmpty());
        QCOMPARE(vm.kind(), 0);
    }

    void testSubmitWithEmptyQueryStaysIdle()
    {
        FakeCinemeta cinemeta;
        SearchViewModel vm(&cinemeta, nullptr);
        vm.setQuery(QStringLiteral("   "));
        vm.submit();
        drain();
        QCOMPARE(cinemeta.searchCalls, 0);
        QCOMPARE(vm.results()->state(),
            ResultsListModel::State::Idle);
    }

    void testSuccessfulQueryPopulatesResults()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {
            makeRow(QStringLiteral("tt0111161"),
                QStringLiteral("The Shawshank Redemption")),
        };
        SearchViewModel vm(&cinemeta, nullptr);
        vm.setQuery(QStringLiteral("shawshank"));

        QSignalSpy statusSpy(&vm, &SearchViewModel::statusMessage);

        vm.submit();
        drain();

        QCOMPARE(cinemeta.searchCalls, 1);
        QCOMPARE(cinemeta.lastQuery, QStringLiteral("shawshank"));
        QCOMPARE(vm.results()->state(),
            ResultsListModel::State::Results);
        QCOMPARE(vm.results()->rowCount(), 1);
        // At least the "Searching…" + "1 result" status messages.
        QVERIFY(statusSpy.count() >= 2);
    }

    void testEmptyResponseFlipsToEmpty()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {};
        SearchViewModel vm(&cinemeta, nullptr);
        vm.setQuery(QStringLiteral("nothing-matches"));
        vm.submit();
        drain();
        QCOMPARE(vm.results()->state(),
            ResultsListModel::State::Empty);
        QCOMPARE(vm.results()->rowCount(), 0);
    }

    void testImdbIdShortcutUsesMeta()
    {
        FakeCinemeta cinemeta;
        MetaDetail d;
        d.summary = makeRow(QStringLiteral("tt0111161"),
            QStringLiteral("Shawshank"));
        cinemeta.cannedMeta = d;

        SearchViewModel vm(&cinemeta, nullptr);
        vm.setQuery(QStringLiteral("tt0111161"));
        vm.submit();
        drain();

        QCOMPARE(cinemeta.metaCalls, 1);
        QCOMPARE(cinemeta.searchCalls, 0);
        QCOMPARE(vm.results()->state(),
            ResultsListModel::State::Results);
        QCOMPARE(vm.results()->rowCount(), 1);
    }

    void testNonImdbQueryUsesSearch()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch
            = { makeRow(QStringLiteral("tt1"), QStringLiteral("X")) };
        SearchViewModel vm(&cinemeta, nullptr);
        // 'tt' alone (no digits) shouldn't trigger the IMDB-id
        // shortcut — same regression-guard the widget controller
        // had.
        vm.setQuery(QStringLiteral("tt"));
        vm.submit();
        drain();
        QCOMPARE(cinemeta.metaCalls, 0);
        QCOMPARE(cinemeta.searchCalls, 1);
    }

    void testErrorPathFlipsToError()
    {
        FakeCinemeta cinemeta;
        cinemeta.throwHttpError = true;
        SearchViewModel vm(&cinemeta, nullptr);
        vm.setQuery(QStringLiteral("anything"));
        vm.submit();
        drain();
        QCOMPARE(vm.results()->state(),
            ResultsListModel::State::Error);
        QVERIFY(!vm.results()->errorMessage().isEmpty());
    }

    void testKindFlipsBetweenMovieAndSeries()
    {
        FakeCinemeta cinemeta;
        SearchViewModel vm(&cinemeta, nullptr);
        QSignalSpy kindSpy(&vm, &SearchViewModel::kindChanged);
        vm.setKind(static_cast<int>(MediaKind::Series));
        QCOMPARE(vm.kind(), static_cast<int>(MediaKind::Series));
        QCOMPARE(kindSpy.count(), 1);

        vm.setQuery(QStringLiteral("series query"));
        vm.submit();
        drain();
        QCOMPARE(cinemeta.lastKind, MediaKind::Series);
    }

    void testActivateRoutesByKind()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {
            makeRow(QStringLiteral("tt1"), QStringLiteral("Movie"),
                MediaKind::Movie),
            makeRow(QStringLiteral("tt2"), QStringLiteral("Show"),
                MediaKind::Series),
        };
        SearchViewModel vm(&cinemeta, nullptr);
        QSignalSpy movieSpy(&vm,
            &SearchViewModel::openMovieRequested);
        QSignalSpy seriesSpy(&vm,
            &SearchViewModel::openSeriesRequested);

        vm.setQuery(QStringLiteral("any"));
        vm.submit();
        drain();

        vm.activate(0);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(movieSpy.first().at(0).toString(),
            QStringLiteral("tt1"));

        vm.activate(1);
        QCOMPARE(seriesSpy.count(), 1);
        QCOMPARE(seriesSpy.first().at(0).toString(),
            QStringLiteral("tt2"));

        // Out-of-range row is a silent no-op.
        vm.activate(99);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(seriesSpy.count(), 1);
    }

    void testStaleResponseDiscarded()
    {
        // Two submits in quick succession: the first epoch is bumped
        // before its co_await completes, so when the canned response
        // lands the post-await guard drops it.
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch
            = { makeRow(QStringLiteral("tt1"),
                QStringLiteral("First")) };
        SearchViewModel vm(&cinemeta, nullptr);

        vm.setQuery(QStringLiteral("first"));
        vm.submit();
        // Don't drain — second submit supersedes.
        cinemeta.cannedSearch
            = { makeRow(QStringLiteral("tt2"),
                QStringLiteral("Second")) };
        vm.setQuery(QStringLiteral("second"));
        vm.submit();
        drain();

        // Whichever drain order Qt picked, only the second call's
        // payload should land in the model.
        QCOMPARE(vm.results()->rowCount(), 1);
        const auto* row = vm.results()->at(0);
        QVERIFY(row != nullptr);
        QCOMPARE(row->imdbId, QStringLiteral("tt2"));
    }

    void testClearReturnsToIdle()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch
            = { makeRow(QStringLiteral("tt1"),
                QStringLiteral("Anything")) };
        SearchViewModel vm(&cinemeta, nullptr);
        vm.setQuery(QStringLiteral("foo"));
        vm.submit();
        drain();
        QCOMPARE(vm.results()->state(),
            ResultsListModel::State::Results);

        vm.clear();
        QVERIFY(vm.query().isEmpty());
        QCOMPARE(vm.results()->state(),
            ResultsListModel::State::Idle);
    }
};

QTEST_MAIN(TstSearchViewModel)
#include "tst_search_view_model.moc"
