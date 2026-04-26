// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/EpisodesListModel.h"

#include <QSignalSpy>
#include <QTest>

using kinema::api::Episode;
using kinema::ui::qml::EpisodesListModel;

namespace {

Episode makeEp(int season, int number, const QString& title,
    std::optional<QDate> released = std::nullopt)
{
    Episode e;
    e.season = season;
    e.number = number;
    e.title = title;
    e.description = QStringLiteral("desc");
    e.released = released;
    e.thumbnail = QUrl(QStringLiteral("https://still/%1.jpg").arg(number));
    return e;
}

} // namespace

class TstEpisodesListModel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testInitialEmpty()
    {
        EpisodesListModel m;
        QCOMPARE(m.rowCount(), 0);
    }

    void testSetEpisodesPopulates()
    {
        EpisodesListModel m;
        QSignalSpy countSpy(&m, &EpisodesListModel::countChanged);
        m.setEpisodes({
            makeEp(1, 1, QStringLiteral("Pilot")),
            makeEp(1, 2, QStringLiteral("Cat's in the Bag")),
        });
        QCOMPARE(m.rowCount(), 2);
        QCOMPARE(countSpy.count(), 1);

        const auto* ep = m.at(1);
        QVERIFY(ep != nullptr);
        QCOMPARE(ep->number, 2);
        QCOMPARE(ep->title, QStringLiteral("Cat's in the Bag"));
    }

    void testNamedRoles()
    {
        EpisodesListModel m;
        m.setEpisodes({
            makeEp(2, 7, QStringLiteral("Hand of the King"),
                QDate(2030, 5, 1)),
        });
        const auto names = m.roleNames();
        for (const auto& key : { QByteArrayLiteral("number"),
                                 QByteArrayLiteral("title"),
                                 QByteArrayLiteral("description"),
                                 QByteArrayLiteral("releasedText"),
                                 QByteArrayLiteral("isUpcoming"),
                                 QByteArrayLiteral("thumbnailUrl"),
                                 QByteArrayLiteral("episode") }) {
            QVERIFY2(names.values().contains(key), key.constData());
        }

        const auto idx = m.index(0);
        QCOMPARE(m.data(idx, EpisodesListModel::NumberRole).toInt(), 7);
        QVERIFY(m.data(idx, EpisodesListModel::IsUpcomingRole).toBool());
        QVERIFY(!m.data(idx, EpisodesListModel::ReleasedTextRole)
                     .toString().isEmpty());
    }

    void testRowFor()
    {
        EpisodesListModel m;
        m.setEpisodes({
            makeEp(1, 1, QStringLiteral("A")),
            makeEp(1, 2, QStringLiteral("B")),
            makeEp(1, 3, QStringLiteral("C")),
        });
        QCOMPARE(m.rowFor(1, 2), 1);
        QCOMPARE(m.rowFor(1, 99), -1);
        QCOMPARE(m.rowFor(2, 1), -1);
    }

    void testThumbnailUrlIsString()
    {
        EpisodesListModel m;
        m.setEpisodes({ makeEp(1, 1, QStringLiteral("Pilot")) });
        const auto idx = m.index(0);
        const auto v = m.data(idx, EpisodesListModel::ThumbnailUrlRole);
        QCOMPARE(v.typeId(), QMetaType::QString);
    }
};

QTEST_MAIN(TstEpisodesListModel)
#include "tst_episodes_list_model.moc"
