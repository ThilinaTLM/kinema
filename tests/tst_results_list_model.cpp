// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Media.h"
#include "ui/qml-bridge/ResultsListModel.h"

#include <QSignalSpy>
#include <QTest>

using kinema::api::MediaKind;
using kinema::api::MetaSummary;
using kinema::ui::qml::ResultsListModel;

namespace {

MetaSummary makeRow(const QString& imdb, const QString& title,
    int year, MediaKind kind = MediaKind::Movie)
{
    MetaSummary s;
    s.imdbId = imdb;
    s.title = title;
    s.year = year;
    s.kind = kind;
    s.poster = QUrl(QStringLiteral(
        "https://images.metahub.space/poster/medium/%1/img").arg(imdb));
    s.imdbRating = 8.4;
    s.description = QStringLiteral("desc");
    return s;
}

} // namespace

class TstResultsListModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testInitialStateIsIdle()
    {
        ResultsListModel m;
        QCOMPARE(m.state(), ResultsListModel::State::Idle);
        QCOMPARE(m.rowCount(), 0);
        QVERIFY(m.errorMessage().isEmpty());
    }

    void testSetLoadingTransitionsAndClearsError()
    {
        ResultsListModel m;
        m.setError(QStringLiteral("oops"));
        QCOMPARE(m.state(), ResultsListModel::State::Error);

        QSignalSpy stateSpy(&m, &ResultsListModel::stateChanged);
        QSignalSpy errSpy(&m, &ResultsListModel::errorMessageChanged);

        m.setLoading();
        QCOMPARE(m.state(), ResultsListModel::State::Loading);
        QVERIFY(m.errorMessage().isEmpty());
        QCOMPARE(stateSpy.count(), 1);
        QCOMPARE(errSpy.count(), 1);
    }

    void testSetResultsPopulatesAndFlipsToResults()
    {
        ResultsListModel m;
        m.setLoading();

        QSignalSpy countSpy(&m, &ResultsListModel::countChanged);
        m.setResults({
            makeRow(QStringLiteral("tt1"), QStringLiteral("A"), 2020),
            makeRow(QStringLiteral("tt2"), QStringLiteral("B"), 2021,
                MediaKind::Series),
        });

        QCOMPARE(m.rowCount(), 2);
        QCOMPARE(m.state(), ResultsListModel::State::Results);
        QCOMPARE(countSpy.count(), 1);

        const auto* first = m.at(0);
        QVERIFY(first != nullptr);
        QCOMPARE(first->imdbId, QStringLiteral("tt1"));
        QCOMPARE(first->kind, MediaKind::Movie);
    }

    void testEmptyResultsFlipsToEmptyState()
    {
        ResultsListModel m;
        m.setLoading();
        m.setResults({});
        QCOMPARE(m.state(), ResultsListModel::State::Empty);
        QCOMPARE(m.rowCount(), 0);
    }

    void testSetErrorClearsRowsAndSetsMessage()
    {
        ResultsListModel m;
        m.setResults({
            makeRow(QStringLiteral("tt1"), QStringLiteral("A"), 2020),
        });
        QCOMPARE(m.rowCount(), 1);

        QSignalSpy errSpy(&m, &ResultsListModel::errorMessageChanged);
        m.setError(QStringLiteral("network down"));
        QCOMPARE(m.state(), ResultsListModel::State::Error);
        QCOMPARE(m.errorMessage(), QStringLiteral("network down"));
        QCOMPARE(m.rowCount(), 0);
        QCOMPARE(errSpy.count(), 1);
    }

    void testRoleNamesExposeQmlFriendlyKeys()
    {
        ResultsListModel m;
        const auto names = m.roleNames();
        QVERIFY(names.values().contains(QByteArrayLiteral("imdbId")));
        QVERIFY(names.values().contains(QByteArrayLiteral("posterUrl")));
        QVERIFY(names.values().contains(QByteArrayLiteral("title")));
        QVERIFY(names.values().contains(QByteArrayLiteral("year")));
        QVERIFY(names.values().contains(QByteArrayLiteral("rating")));
        QVERIFY(names.values().contains(QByteArrayLiteral("kind")));
    }

    void testPosterRoleReturnsString()
    {
        // The QML delegate `encodeURIComponent`s the URL string;
        // returning a raw QUrl breaks that path with type-coercion
        // surprises. Pin the contract in a test.
        ResultsListModel m;
        m.setResults({
            makeRow(QStringLiteral("tt1"), QStringLiteral("A"), 2020),
        });
        const auto v = m.data(m.index(0),
            ResultsListModel::PosterUrlRole);
        QCOMPARE(v.metaType().id(), QMetaType::QString);
        QVERIFY(v.toString().startsWith(QStringLiteral("https://")));
    }

    void testSetIdleClearsRowsAndState()
    {
        ResultsListModel m;
        m.setResults({
            makeRow(QStringLiteral("tt1"), QStringLiteral("A"), 2020),
        });
        m.setIdle();
        QCOMPARE(m.state(), ResultsListModel::State::Idle);
        QCOMPARE(m.rowCount(), 0);
    }
};

QTEST_MAIN(TstResultsListModel)
#include "tst_results_list_model.moc"
