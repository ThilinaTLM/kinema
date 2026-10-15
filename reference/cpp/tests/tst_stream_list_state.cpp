// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/FilterSettings.h"
#include "ui/qml-bridge/streams/StreamListState.h"

#include <QSignalSpy>
#include <QStandardPaths>
#include <QTest>

#include <KSharedConfig>

using namespace kinema;

namespace {

domain::Stream stream(QString name, QString resolution, int seeders)
{
    domain::Stream value;
    value.releaseName = std::move(name);
    value.resolution = std::move(resolution);
    value.seeders = seeders;
    return value;
}

} // namespace

class TstStreamListState : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
        m_config = KSharedConfig::openConfig(QStringLiteral("kinemarc-stream-list-state-test"),
                                             KConfig::SimpleConfig);
        m_filters = std::make_unique<config::FilterSettings>(m_config);
        m_filters->setExcludedResolutions({});
        m_filters->setExcludedCategories({});
        m_filters->setKeywordBlocklist({});
    }

    void sortsAndFiltersRows()
    {
        ui::qml::StreamListState state(*m_filters, domain::MediaKind::Movie);
        state.setRawStreams({
            stream(QStringLiteral("Movie.720p"), QStringLiteral("720p"), 50),
            stream(QStringLiteral("Movie.2160p.HDR"), QStringLiteral("2160p"), 2),
        });
        QCOMPARE(state.rawStreamsCount(), 2);
        QCOMPARE(state.model()->rowCount(), 2);
        QCOMPARE(state.model()->at(0)->resolution, QStringLiteral("2160p"));

        state.setResolutionFilter(QStringLiteral("720p"));
        QCOMPARE(state.model()->rowCount(), 1);
        QCOMPARE(state.model()->at(0)->resolution, QStringLiteral("720p"));

        state.clearUiFilters();
        QCOMPARE(state.model()->rowCount(), 2);
        state.setHdrOnly(true);
        QCOMPARE(state.model()->rowCount(), 1);
        QCOMPARE(state.model()->at(0)->resolution, QStringLiteral("2160p"));
    }

    void reactsToPersistentBlocklist()
    {
        ui::qml::StreamListState state(*m_filters, domain::MediaKind::Movie);
        state.setRawStreams({
            stream(QStringLiteral("Good.Release"), QStringLiteral("1080p"), 1),
            stream(QStringLiteral("Blocked.Release"), QStringLiteral("1080p"), 2),
        });
        m_filters->setKeywordBlocklist({QStringLiteral("blocked")});
        QCOMPARE(state.model()->rowCount(), 1);
        QCOMPARE(state.model()->at(0)->releaseName, QStringLiteral("Good.Release"));
        m_filters->setKeywordBlocklist({});
    }

    void keepsMovieAndSeriesPackClassificationSeparate()
    {
        const auto pack =
            stream(QStringLiteral("Show.S01.Complete.1080p"), QStringLiteral("1080p"), 1);
        ui::qml::StreamListState movie(*m_filters, domain::MediaKind::Movie);
        ui::qml::StreamListState series(*m_filters, domain::MediaKind::Series);
        movie.setRawStreams({pack});
        series.setRawStreams({pack});

        const auto movieKind =
            movie.model()->data(movie.model()->index(0), ui::qml::StreamsListModel::PackKindRole);
        const auto seriesKind =
            series.model()->data(series.model()->index(0), ui::qml::StreamsListModel::PackKindRole);
        QCOMPARE(movieKind.toString(), QStringLiteral("none"));
        QVERIFY(seriesKind.toString() != QStringLiteral("none"));
    }

private:
    KSharedConfigPtr m_config;
    std::unique_ptr<config::FilterSettings> m_filters;
};

QTEST_GUILESS_MAIN(TstStreamListState)
#include "tst_stream_list_state.moc"
