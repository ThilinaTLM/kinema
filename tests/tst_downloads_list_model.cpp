// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DownloadsListModel.h"

#include "domain/Download.h"

#include <QSignalSpy>
#include <QTest>

using kinema::domain::DownloadItem;
using kinema::domain::DownloadMode;
using kinema::domain::DownloadState;
using kinema::ui::qml::DownloadsListModel;

namespace {

DownloadItem makeRow(const QString& assetId, const QString& title,
    DownloadState state = DownloadState::Active)
{
    DownloadItem it;
    it.assetId = assetId;
    it.title = title;
    it.state = state;
    it.mode = DownloadMode::Full;
    it.cachedSizeBytes = 100;
    it.expectedSizeBytes = 1000;
    it.complete = false;
    return it;
}

} // namespace

class TstDownloadsListModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void setItemsPopulatesAssetIndex()
    {
        DownloadsListModel m;
        QSignalSpy resetSpy(&m, &DownloadsListModel::modelReset);
        m.setItems({
            makeRow(QStringLiteral("a"), QStringLiteral("Alpha")),
            makeRow(QStringLiteral("b"), QStringLiteral("Beta")),
        });
        QCOMPARE(m.rowCount(), 2);
        QCOMPARE(resetSpy.count(), 1);
        QCOMPARE(m.rowForAssetId(QStringLiteral("a")), 0);
        QCOMPARE(m.rowForAssetId(QStringLiteral("b")), 1);
        QCOMPARE(m.rowForAssetId(QStringLiteral("nope")), -1);
    }

    void updateLiveStatsForEmitsDataChangedWithoutReset()
    {
        DownloadsListModel m;
        m.setItems({
            makeRow(QStringLiteral("a"), QStringLiteral("Alpha")),
            makeRow(QStringLiteral("b"), QStringLiteral("Beta")),
        });

        QSignalSpy resetSpy(&m, &DownloadsListModel::modelReset);
        QSignalSpy aboutToResetSpy(&m,
            &DownloadsListModel::modelAboutToBeReset);
        QSignalSpy dataSpy(&m, &DownloadsListModel::dataChanged);

        DownloadsListModel::LiveRow lr;
        lr.ratePayloadBps = 1234;
        lr.peers = 5;
        lr.seeds = 2;
        lr.etaSeconds = 60;
        m.updateLiveStatsFor(QStringLiteral("b"), lr);

        QCOMPARE(resetSpy.count(), 0);
        QCOMPARE(aboutToResetSpy.count(), 0);
        QCOMPARE(dataSpy.count(), 1);

        const auto args = dataSpy.takeFirst();
        const auto top = args.at(0).toModelIndex();
        const auto bottom = args.at(1).toModelIndex();
        QCOMPARE(top.row(), 1);
        QCOMPARE(bottom.row(), 1);
        const auto roles = args.at(2).value<QVector<int>>();
        QVERIFY(roles.contains(DownloadsListModel::DownloadRateBpsRole));
        QVERIFY(roles.contains(DownloadsListModel::DownloadRateTextRole));
        QVERIFY(roles.contains(DownloadsListModel::PeersRole));
        QVERIFY(roles.contains(DownloadsListModel::SeedsRole));
        QVERIFY(roles.contains(DownloadsListModel::EtaSecondsRole));
        QVERIFY(roles.contains(DownloadsListModel::EtaTextRole));
        // Live-only update must not advertise persistent roles.
        QVERIFY(!roles.contains(DownloadsListModel::StateRole));
        QVERIFY(!roles.contains(DownloadsListModel::ProgressFractionRole));

        const auto idx = m.index(1);
        QCOMPARE(m.data(idx,
                     DownloadsListModel::DownloadRateBpsRole).toLongLong(),
            qint64(1234));
        QCOMPARE(m.data(idx, DownloadsListModel::PeersRole).toInt(), 5);
    }

    void updateRowEmitsDataChangedWithoutReset()
    {
        DownloadsListModel m;
        m.setItems({
            makeRow(QStringLiteral("a"), QStringLiteral("Alpha"),
                DownloadState::Active),
        });

        QSignalSpy resetSpy(&m, &DownloadsListModel::modelReset);
        QSignalSpy dataSpy(&m, &DownloadsListModel::dataChanged);

        auto fresh = makeRow(QStringLiteral("a"), QStringLiteral("Alpha"),
            DownloadState::Paused);
        fresh.cachedSizeBytes = 500;
        m.updateRow(QStringLiteral("a"), fresh);

        QCOMPARE(resetSpy.count(), 0);
        QCOMPARE(dataSpy.count(), 1);

        const auto args = dataSpy.takeFirst();
        const auto roles = args.at(2).value<QVector<int>>();
        QVERIFY(roles.contains(DownloadsListModel::StateRole));
        QVERIFY(roles.contains(DownloadsListModel::StateTextRole));
        QVERIFY(roles.contains(DownloadsListModel::ProgressFractionRole));
        QVERIFY(roles.contains(DownloadsListModel::CachedSizeBytesRole));
        // Persistent update must not advertise live-only roles.
        QVERIFY(!roles.contains(DownloadsListModel::DownloadRateBpsRole));
        QVERIFY(!roles.contains(DownloadsListModel::PeersRole));

        const auto idx = m.index(0);
        QCOMPARE(m.data(idx, DownloadsListModel::StateRole).toInt(),
            static_cast<int>(DownloadState::Paused));
        QCOMPARE(
            m.data(idx, DownloadsListModel::CachedSizeBytesRole).toLongLong(),
            qint64(500));
    }

    void updateForUnknownAssetIdIsNoOp()
    {
        DownloadsListModel m;
        m.setItems({ makeRow(QStringLiteral("a"), QStringLiteral("Alpha")) });

        QSignalSpy resetSpy(&m, &DownloadsListModel::modelReset);
        QSignalSpy dataSpy(&m, &DownloadsListModel::dataChanged);

        m.updateLiveStatsFor(QStringLiteral("nope"), {});
        m.updateRow(QStringLiteral("nope"),
            makeRow(QStringLiteral("nope"), QStringLiteral("Nope")));

        QCOMPARE(resetSpy.count(), 0);
        QCOMPARE(dataSpy.count(), 0);
    }

    void updateAttachedPlayersEmitsForFlippedRowsOnly()
    {
        DownloadsListModel m;
        m.setItems({
            makeRow(QStringLiteral("a"), QStringLiteral("Alpha")),
            makeRow(QStringLiteral("b"), QStringLiteral("Beta")),
            makeRow(QStringLiteral("c"), QStringLiteral("Gamma")),
        }, {}, { QStringLiteral("a") });

        QSignalSpy resetSpy(&m, &DownloadsListModel::modelReset);
        QSignalSpy dataSpy(&m, &DownloadsListModel::dataChanged);

        // a was attached, b becomes attached, c stays detached.
        m.updateAttachedPlayers({ QStringLiteral("b") });

        QCOMPARE(resetSpy.count(), 0);
        // One emission for a (true -> false), one for b (false -> true).
        QCOMPARE(dataSpy.count(), 2);

        QSet<int> flippedRows;
        for (int i = 0; i < dataSpy.count(); ++i) {
            const auto args = dataSpy.at(i);
            const auto top = args.at(0).toModelIndex();
            flippedRows.insert(top.row());
            const auto roles = args.at(2).value<QVector<int>>();
            QVERIFY(roles.contains(
                DownloadsListModel::HasPlayerAttachedRole));
            QVERIFY(roles.contains(DownloadsListModel::StateTextRole));
        }
        QVERIFY(flippedRows.contains(0));
        QVERIFY(flippedRows.contains(1));
        QVERIFY(!flippedRows.contains(2));

        const auto rowA = m.index(0);
        const auto rowB = m.index(1);
        QVERIFY(!m.data(rowA,
                      DownloadsListModel::HasPlayerAttachedRole).toBool());
        QVERIFY(m.data(rowB,
                    DownloadsListModel::HasPlayerAttachedRole).toBool());
    }

    void setItemsResetsAssetIndexAcrossCalls()
    {
        DownloadsListModel m;
        m.setItems({
            makeRow(QStringLiteral("a"), QStringLiteral("Alpha")),
            makeRow(QStringLiteral("b"), QStringLiteral("Beta")),
        });
        QCOMPARE(m.rowForAssetId(QStringLiteral("a")), 0);

        // Replace with a different set; stale entries must be gone.
        m.setItems({
            makeRow(QStringLiteral("c"), QStringLiteral("Gamma")),
        });
        QCOMPARE(m.rowCount(), 1);
        QCOMPARE(m.rowForAssetId(QStringLiteral("a")), -1);
        QCOMPARE(m.rowForAssetId(QStringLiteral("c")), 0);
    }
};

QTEST_MAIN(TstDownloadsListModel)
#include "tst_downloads_list_model.moc"
