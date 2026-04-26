// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/StreamsListModel.h"

#include <QSignalSpy>
#include <QTest>

using kinema::api::Stream;
using kinema::ui::qml::StreamsListModel;

namespace {

Stream makeStream(const QString& releaseName,
    const QString& resolution,
    const QString& provider,
    qint64 size,
    int seeders,
    bool rdCached = false)
{
    Stream s;
    s.releaseName = releaseName;
    s.resolution = resolution;
    s.provider = provider;
    s.sizeBytes = size;
    s.seeders = seeders;
    s.rdCached = rdCached;
    s.infoHash = QStringLiteral("0123456789abcdef");
    return s;
}

} // namespace

class TstStreamsListModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testInitialState()
    {
        StreamsListModel m;
        QCOMPARE(m.state(), StreamsListModel::State::Idle);
        QVERIFY(m.errorMessage().isEmpty());
        QVERIFY(m.emptyExplanation().isEmpty());
        QCOMPARE(m.rowCount(), 0);
        QVERIFY(!m.releaseDate().isValid());
    }

    void testSetItemsReadyAndEmpty()
    {
        StreamsListModel m;
        m.setItems({ makeStream(QStringLiteral("Foo.1080p"),
            QStringLiteral("1080p"),
            QStringLiteral("rargb"), 1'000'000, 5) });
        QCOMPARE(m.state(), StreamsListModel::State::Ready);
        QCOMPARE(m.rowCount(), 1);

        m.setItems({});
        QCOMPARE(m.state(), StreamsListModel::State::Empty);
        QCOMPARE(m.rowCount(), 0);
    }

    void testSetItemsEmptyExplanation()
    {
        StreamsListModel m;
        m.setItems({}, QStringLiteral("filtered out"));
        QCOMPARE(m.state(), StreamsListModel::State::Empty);
        QCOMPARE(m.emptyExplanation(),
            QStringLiteral("filtered out"));

        // A successful `setItems` clears the explanation again.
        m.setItems({ makeStream(QStringLiteral("Bar.720p"),
            QStringLiteral("720p"),
            QStringLiteral("eztv"), 5'000'000, 1) });
        QVERIFY(m.emptyExplanation().isEmpty());
    }

    void testSetLoadingClearsError()
    {
        StreamsListModel m;
        m.setError(QStringLiteral("network"));
        QCOMPARE(m.state(), StreamsListModel::State::Error);
        QCOMPARE(m.errorMessage(), QStringLiteral("network"));

        m.setLoading();
        QCOMPARE(m.state(), StreamsListModel::State::Loading);
        QVERIFY(m.errorMessage().isEmpty());
    }

    void testSetError()
    {
        StreamsListModel m;
        m.setItems({ makeStream(QStringLiteral("X"),
            QStringLiteral("720p"),
            QStringLiteral("p"), 1, 1) });
        QSignalSpy stateSpy(&m, &StreamsListModel::stateChanged);

        m.setError(QStringLiteral("torrentio down"));
        QCOMPARE(m.state(), StreamsListModel::State::Error);
        QCOMPARE(m.errorMessage(), QStringLiteral("torrentio down"));
        QCOMPARE(m.rowCount(), 0);
        QVERIFY(stateSpy.count() >= 1);
    }

    void testSetUnreleased()
    {
        StreamsListModel m;
        const auto when = QDate(2030, 6, 1);
        m.setUnreleased(when);
        QCOMPARE(m.state(), StreamsListModel::State::Unreleased);
        QCOMPARE(m.releaseDate(), when);
        QCOMPARE(m.rowCount(), 0);
    }

    void testSetIdleClearsAllSideChannels()
    {
        StreamsListModel m;
        m.setItems({ makeStream(QStringLiteral("X"),
            QStringLiteral("1080p"),
            QStringLiteral("p"), 1, 1) });
        m.setError(QStringLiteral("e"));
        m.setUnreleased(QDate(2099, 1, 1));

        m.setIdle();
        QCOMPARE(m.state(), StreamsListModel::State::Idle);
        QVERIFY(m.errorMessage().isEmpty());
        QVERIFY(m.emptyExplanation().isEmpty());
        QVERIFY(!m.releaseDate().isValid());
        QCOMPARE(m.rowCount(), 0);
    }

    void testRoleNamesContainExpectedKeys()
    {
        StreamsListModel m;
        const auto names = m.roleNames();
        for (const auto& key : { QByteArrayLiteral("releaseName"),
                                 QByteArrayLiteral("resolution"),
                                 QByteArrayLiteral("qualityLabel"),
                                 QByteArrayLiteral("sizeText"),
                                 QByteArrayLiteral("seeders"),
                                 QByteArrayLiteral("provider"),
                                 QByteArrayLiteral("hasMagnet"),
                                 QByteArrayLiteral("hasDirectUrl"),
                                 QByteArrayLiteral("rdCached"),
                                 QByteArrayLiteral("chips") }) {
            QVERIFY2(names.values().contains(key),
                key.constData());
        }
    }

    void testChipsForBuildsExpectedList()
    {
        const auto raw = makeStream(QStringLiteral("X"),
            QStringLiteral("1080p"),
            QStringLiteral("rargb"), 1, 5);
        const auto chips = StreamsListModel::chipsFor(raw);
        // Expected: resolution + provider, no RD chip when not flagged.
        QVERIFY(chips.contains(QStringLiteral("1080p")));
        QVERIFY(chips.contains(QStringLiteral("rargb")));
        QCOMPARE(chips.size(), 2);

        auto rd = raw;
        rd.rdCached = true;
        const auto rdChips = StreamsListModel::chipsFor(rd);
        QVERIFY(rdChips.contains(QStringLiteral("RD+")));
    }

    void testFormatSize()
    {
        QCOMPARE(StreamsListModel::formatSize(std::nullopt),
            QStringLiteral("\u2014"));
        QCOMPARE(StreamsListModel::formatSize(0),
            QStringLiteral("\u2014"));
        QVERIFY(!StreamsListModel::formatSize(1'500'000'000).isEmpty());
    }

    void testQualityLabelRoleAddsRdSuffix()
    {
        StreamsListModel m;
        auto rd = makeStream(QStringLiteral("X"),
            QStringLiteral("1080p"),
            QStringLiteral("p"), 1, 1, /*rdCached=*/true);
        m.setItems({ rd });
        const auto label = m.data(m.index(0),
            StreamsListModel::QualityLabelRole).toString();
        QVERIFY2(label.contains(QStringLiteral("[RD+]")),
            qPrintable(label));
    }
};

QTEST_MAIN(TstStreamsListModel)
#include "tst_streams_list_model.moc"
