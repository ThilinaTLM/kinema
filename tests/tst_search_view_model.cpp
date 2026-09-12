// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/cinemeta/CinemetaClient.h"
#include "api/tmdb/TmdbClient.h"
#include "config/SearchSettings.h"
#include "core/io/HttpError.h"
#include "domain/Discover.h"
#include "domain/Media.h"
#include "ui/qml-bridge/search/ResultsListModel.h"
#include "ui/qml-bridge/search/SearchViewModel.h"

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include <KConfig>
#include <KSharedConfig>

using kinema::api::CinemetaClient;
using kinema::api::TmdbClient;
using kinema::config::SearchSettings;
using kinema::core::HttpError;
using kinema::domain::DiscoverItem;
using kinema::domain::DiscoverPageResult;
using kinema::domain::MediaKind;
using kinema::domain::MetaDetail;
using kinema::domain::MetaSummary;
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
    FakeCinemeta() : CinemetaClient(nullptr) { }

    struct MetaReply
    {
        MetaDetail detail;
        bool throwHttpError = false;
    };

    QList<MetaSummary> cannedSearch;
    MetaDetail cannedMeta;
    QList<MetaReply> scriptedMetaReplies;
    bool throwHttpError = false;

    int searchCalls = 0;
    int metaCalls = 0;
    QString lastQuery;
    QString lastImdbId;
    MediaKind lastKind = MediaKind::Movie;
    QList<MediaKind> metaKinds;

    QCoro::Task<QList<MetaSummary>> search(MediaKind kind, QString query) override
    {
        ++searchCalls;
        lastQuery = query;
        lastKind = kind;
        if (throwHttpError) {
            throw HttpError(HttpError::Kind::Network, 0, QStringLiteral("canned failure"));
        }
        co_return cannedSearch;
    }

    QCoro::Task<MetaDetail> meta(MediaKind kind, QString imdbId) override
    {
        ++metaCalls;
        lastKind = kind;
        lastImdbId = imdbId;
        metaKinds.append(kind);
        if (!scriptedMetaReplies.isEmpty()) {
            const auto reply = scriptedMetaReplies.takeFirst();
            if (reply.throwHttpError) {
                throw HttpError(HttpError::Kind::Network, 0, QStringLiteral("canned failure"));
            }
            co_return reply.detail;
        }
        if (throwHttpError) {
            throw HttpError(HttpError::Kind::Network, 0, QStringLiteral("canned failure"));
        }
        co_return cannedMeta;
    }
};

class FakeTmdb : public TmdbClient
{
public:
    FakeTmdb() : TmdbClient(nullptr) { setToken(QStringLiteral("tmdb-token")); }

    DiscoverPageResult cannedSearch;
    QHash<int, QString> movieImdbIds;
    QHash<int, QString> seriesImdbIds;
    bool throwSearchError = false;

    int searchCalls = 0;
    int movieLookupCalls = 0;
    int seriesLookupCalls = 0;
    QString lastQuery;
    MediaKind lastKind = MediaKind::Movie;

    QCoro::Task<DiscoverPageResult> search(MediaKind kind, QString query, int page = 1) override
    {
        Q_UNUSED(page);
        ++searchCalls;
        lastKind = kind;
        lastQuery = query;
        if (throwSearchError) {
            throw HttpError(HttpError::Kind::Network, 0, QStringLiteral("tmdb failure"));
        }
        co_return cannedSearch;
    }

    QCoro::Task<QString> imdbIdForTmdbMovie(int tmdbId) override
    {
        ++movieLookupCalls;
        co_return movieImdbIds.value(tmdbId);
    }

    QCoro::Task<QString> imdbIdForTmdbSeries(int tmdbId) override
    {
        ++seriesLookupCalls;
        co_return seriesImdbIds.value(tmdbId);
    }
};

MetaSummary makeRow(const QString& imdb, const QString& title, MediaKind kind = MediaKind::Movie)
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
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Idle);
        QVERIFY(vm.query().isEmpty());
        QCOMPARE(vm.kind(), 0);
    }

    void testSubmitWithEmptyQueryStaysIdle()
    {
        FakeCinemeta cinemeta;
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        vm.setQuery(QStringLiteral("   "));
        vm.submit();
        drain();
        QCOMPARE(cinemeta.searchCalls, 0);
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Idle);
    }

    void testSuccessfulQueryPopulatesResults()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {
            makeRow(QStringLiteral("tt0111161"), QStringLiteral("The Shawshank Redemption")),
        };
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        vm.setQuery(QStringLiteral("shawshank"));

        QSignalSpy statusSpy(&vm, &SearchViewModel::statusMessage);

        vm.submit();
        drain();

        QCOMPARE(cinemeta.searchCalls, 1);
        QCOMPARE(cinemeta.lastQuery, QStringLiteral("shawshank"));
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);
        QCOMPARE(vm.results()->rowCount(), 1);
        // At least the "Searching…" + "1 result" status messages.
        QVERIFY(statusSpy.count() >= 2);
    }

    void testEmptyResponseFlipsToEmpty()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {};
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        vm.setQuery(QStringLiteral("nothing-matches"));
        vm.submit();
        drain();
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Empty);
        QCOMPARE(vm.results()->rowCount(), 0);
    }

    void testTextSearchFallsBackToTmdbWhenCinemetaEmpty()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {};

        FakeTmdb tmdb;
        DiscoverItem item;
        item.tmdbId = 1041054;
        item.kind = MediaKind::Movie;
        item.title = QStringLiteral("Freddy");
        item.year = 2022;
        item.poster = QUrl(QStringLiteral("https://image.tmdb.org/t/p/w342/freddy.jpg"));
        item.overview = QStringLiteral("A shy dentist becomes obsessed.");
        tmdb.cannedSearch.items = {item};
        tmdb.movieImdbIds.insert(1041054, QStringLiteral("tt15145764"));

        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, &tmdb, settings, nullptr);
        QSignalSpy movieSpy(&vm, &SearchViewModel::openMovieRequested);

        vm.setQuery(QStringLiteral("freddy hindi"));
        vm.submit();
        drain();

        QCOMPARE(cinemeta.searchCalls, 1);
        QCOMPARE(tmdb.searchCalls, 1);
        QCOMPARE(tmdb.movieLookupCalls, 1);
        QCOMPARE(tmdb.lastQuery, QStringLiteral("freddy hindi"));
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);
        QCOMPARE(vm.results()->rowCount(), 1);
        const auto* row = vm.results()->at(0);
        QVERIFY(row != nullptr);
        QCOMPARE(row->imdbId, QStringLiteral("tt15145764"));
        QCOMPARE(row->title, QStringLiteral("Freddy"));

        vm.activate(0);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(movieSpy.first().at(0).toString(), QStringLiteral("tt15145764"));
    }

    void testImdbIdShortcutUsesMeta()
    {
        FakeCinemeta cinemeta;
        MetaDetail d;
        d.summary = makeRow(QStringLiteral("tt0111161"), QStringLiteral("Shawshank"));
        cinemeta.cannedMeta = d;

        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        // IMDB-id detection only picks the endpoint inside
        // runSearchTask; submission itself is always explicit
        // (Enter / Refresh action -> submit()).
        vm.setQuery(QStringLiteral("tt0111161"));
        drain();
        QCOMPARE(cinemeta.metaCalls, 0);
        QCOMPARE(cinemeta.searchCalls, 0);

        vm.submit();
        drain();

        QCOMPARE(cinemeta.metaCalls, 1);
        QCOMPARE(cinemeta.lastImdbId, QStringLiteral("tt0111161"));
        QCOMPARE(cinemeta.searchCalls, 0);
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);
        QCOMPARE(vm.results()->rowCount(), 1);
    }

    void testImdbUrlShortcutUsesMeta()
    {
        FakeCinemeta cinemeta;
        MetaDetail d;
        d.summary = makeRow(QStringLiteral("tt15145764"), QStringLiteral("Freddy"));
        cinemeta.cannedMeta = d;

        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        vm.setQuery(QStringLiteral("https://www.imdb.com/title/tt15145764/?ref_=fn_al_tt_1"));

        vm.submit();
        drain();

        QCOMPARE(cinemeta.metaCalls, 1);
        QCOMPARE(cinemeta.lastImdbId, QStringLiteral("tt15145764"));
        QCOMPARE(cinemeta.searchCalls, 0);
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);
        QCOMPARE(vm.results()->rowCount(), 1);
    }

    void testImdbShortcutFallsBackToOppositeKind()
    {
        FakeCinemeta cinemeta;
        MetaDetail d;
        d.summary =
            makeRow(QStringLiteral("tt15145764"), QStringLiteral("Freddy"), MediaKind::Movie);
        cinemeta.scriptedMetaReplies = {
            {{}, true},
            {d, false},
        };

        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        vm.setKind(static_cast<int>(MediaKind::Series));
        vm.setQuery(QStringLiteral("https://m.imdb.com/title/tt15145764/"));

        vm.submit();
        drain();

        QCOMPARE(cinemeta.metaCalls, 2);
        QCOMPARE(cinemeta.searchCalls, 0);
        QCOMPARE(cinemeta.metaKinds.at(0), MediaKind::Series);
        QCOMPARE(cinemeta.metaKinds.at(1), MediaKind::Movie);
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);
        QCOMPARE(vm.results()->rowCount(), 1);
        const auto* row = vm.results()->at(0);
        QVERIFY(row != nullptr);
        QCOMPARE(row->kind, MediaKind::Movie);
        QCOMPARE(row->title, QStringLiteral("Freddy"));
    }

    void testTypingDoesNotSubmit()
    {
        // Regression guard for the explicit-submit contract:
        // setQuery() mutates text only, never fires a request,
        // even after waiting past the old debounce window.
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {makeRow(QStringLiteral("tt1"), QStringLiteral("X"))};
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);

        vm.setQuery(QStringLiteral("mat"));
        vm.setQuery(QStringLiteral("matri"));
        vm.setQuery(QStringLiteral("matrix"));
        QTest::qWait(400);
        drain();
        QCOMPARE(cinemeta.searchCalls, 0);
        QCOMPARE(cinemeta.metaCalls, 0);
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Idle);
    }

    void testResubmitRerunsSameQuery()
    {
        // The Refresh action's whole point: pressing submit() twice
        // on the same query issues two network calls.
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {makeRow(QStringLiteral("tt1"), QStringLiteral("X"))};
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);

        vm.setQuery(QStringLiteral("matrix"));
        vm.submit();
        drain();
        QCOMPARE(cinemeta.searchCalls, 1);

        vm.submit();
        drain();
        QCOMPARE(cinemeta.searchCalls, 2);
    }

    void testEmptyingFieldKeepsLastResults()
    {
        // Backspacing to empty must not implicitly reset the model;
        // only submit() / clear() / Escape change state. Without
        // this guard, a stale Idle placeholder would replace the
        // grid as soon as the user cleared the text to retype.
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {makeRow(QStringLiteral("tt1"), QStringLiteral("Anything"))};
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);

        vm.setQuery(QStringLiteral("foo"));
        vm.submit();
        drain();
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);

        vm.setQuery(QString());
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);
        QVERIFY(vm.query().isEmpty());
    }

    void testNonImdbQueryUsesSearch()
    {
        FakeCinemeta cinemeta;
        cinemeta.cannedSearch = {makeRow(QStringLiteral("tt1"), QStringLiteral("X"))};
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
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
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        vm.setQuery(QStringLiteral("anything"));
        vm.submit();
        drain();
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Error);
        QVERIFY(!vm.results()->errorMessage().isEmpty());
    }

    void testKindFlipsBetweenMovieAndSeries()
    {
        FakeCinemeta cinemeta;
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
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
            makeRow(QStringLiteral("tt1"), QStringLiteral("Movie"), MediaKind::Movie),
            makeRow(QStringLiteral("tt2"), QStringLiteral("Show"), MediaKind::Series),
        };
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        QSignalSpy movieSpy(&vm, &SearchViewModel::openMovieRequested);
        QSignalSpy seriesSpy(&vm, &SearchViewModel::openSeriesRequested);

        vm.setQuery(QStringLiteral("any"));
        vm.submit();
        drain();

        vm.activate(0);
        QCOMPARE(movieSpy.count(), 1);
        QCOMPARE(movieSpy.first().at(0).toString(), QStringLiteral("tt1"));

        vm.activate(1);
        QCOMPARE(seriesSpy.count(), 1);
        QCOMPARE(seriesSpy.first().at(0).toString(), QStringLiteral("tt2"));

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
        cinemeta.cannedSearch = {makeRow(QStringLiteral("tt1"), QStringLiteral("First"))};
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);

        vm.setQuery(QStringLiteral("first"));
        vm.submit();
        // Don't drain — second submit supersedes.
        cinemeta.cannedSearch = {makeRow(QStringLiteral("tt2"), QStringLiteral("Second"))};
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
        cinemeta.cannedSearch = {makeRow(QStringLiteral("tt1"), QStringLiteral("Anything"))};
        QTemporaryDir tmp;
        auto config = KSharedConfig::openConfig(tmp.filePath(QStringLiteral("kinemarc")),
                                                KConfig::SimpleConfig);
        SearchSettings settings(config);
        SearchViewModel vm(&cinemeta, nullptr, settings, nullptr);
        vm.setQuery(QStringLiteral("foo"));
        vm.submit();
        drain();
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Results);

        vm.clear();
        QVERIFY(vm.query().isEmpty());
        QCOMPARE(vm.results()->state(), ResultsListModel::State::Idle);
    }
};

QTEST_MAIN(TstSearchViewModel)
#include "tst_search_view_model.moc"
