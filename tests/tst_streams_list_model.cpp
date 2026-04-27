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
                                 QByteArrayLiteral("chips"),
                                 QByteArrayLiteral("source"),
                                 QByteArrayLiteral("codec"),
                                 QByteArrayLiteral("hdr"),
                                 QByteArrayLiteral("audioSummary"),
                                 QByteArrayLiteral("languages"),
                                 QByteArrayLiteral("multiAudio"),
                                 QByteArrayLiteral("releaseGroup"),
                                 QByteArrayLiteral("summaryLine"),
                                 QByteArrayLiteral("tags") }) {
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

    void testSummaryLineAndTagsRoles()
    {
        StreamsListModel m;
        Stream s = makeStream(
            QStringLiteral("From.S01.1080p.WEB-DL.x265.10bit.EAC3-QxR"),
            QStringLiteral("1080p"),
            QStringLiteral("TorrentGalaxy"),
            1'500'000'000, 42, /*rdCached=*/true);
        m.setItems({ s });

        const auto summary = m.data(m.index(0),
            StreamsListModel::SummaryLineRole).toString();
        QVERIFY2(summary.contains(QStringLiteral("WEB-DL")), qPrintable(summary));
        QVERIFY2(summary.contains(QStringLiteral("x265 10-bit")),
            qPrintable(summary));
        QVERIFY2(summary.contains(QStringLiteral("EAC3")), qPrintable(summary));

        const auto tags = m.data(m.index(0),
            StreamsListModel::TagsRole).toStringList();
        // Tags do NOT include resolution or RD — those have dedicated
        // visual treatment in the leading quality block.
        QVERIFY(!tags.contains(QStringLiteral("1080p")));
        QVERIFY(!tags.contains(QStringLiteral("RD+")));
        // Tags DO include codec, release group.
        QVERIFY(tags.contains(QStringLiteral("x265 10-bit")));
        QVERIFY(tags.contains(QStringLiteral("QxR")));
    }

    void testEmptySummaryWhenNoTokens()
    {
        StreamsListModel m;
        m.setItems({ makeStream(QStringLiteral("NoMetadataHere"),
            QStringLiteral("\u2014"),
            QString {}, 1, 0) });
        const auto summary = m.data(m.index(0),
            StreamsListModel::SummaryLineRole).toString();
        QVERIFY(summary.isEmpty());
    }

    void testTokenCacheClearsOnSetItems()
    {
        StreamsListModel m;
        m.setItems({ makeStream(
            QStringLiteral("A.1080p.WEB-DL.x265"),
            QStringLiteral("1080p"), QStringLiteral("p"), 1, 1) });
        const auto first = m.data(m.index(0),
            StreamsListModel::SummaryLineRole).toString();
        QVERIFY(first.contains(QStringLiteral("x265")));

        // Replace with a different release — the cache must be wiped
        // so role 0 reflects the new row's tokens, not the old.
        m.setItems({ makeStream(
            QStringLiteral("B.1080p.BluRay.x264"),
            QStringLiteral("1080p"), QStringLiteral("p"), 1, 1) });
        const auto second = m.data(m.index(0),
            StreamsListModel::SummaryLineRole).toString();
        QVERIFY(second.contains(QStringLiteral("x264")));
        QVERIFY(!second.contains(QStringLiteral("x265")));
    }

    void testSortModeEnumOrder()
    {
        // Smart is the new default and lives at index 0; the rest
        // shifted up by one. Tests / QML that read the enum by value
        // rather than name need to know.
        QCOMPARE(static_cast<int>(StreamsListModel::SortMode::Smart), 0);
        QCOMPARE(static_cast<int>(StreamsListModel::SortMode::Seeders), 1);
        QCOMPARE(static_cast<int>(StreamsListModel::SortMode::Size), 2);
        QCOMPARE(static_cast<int>(StreamsListModel::SortMode::Quality), 3);
        QCOMPARE(static_cast<int>(StreamsListModel::SortMode::Provider), 4);
        QCOMPARE(static_cast<int>(StreamsListModel::SortMode::ReleaseName), 5);
    }
};

QTEST_MAIN(TstStreamsListModel)
#include "tst_streams_list_model.moc"
