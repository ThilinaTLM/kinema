// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/ChaptersModel.h"

#include <QTest>

using kinema::core::chapters::Chapter;
using kinema::core::chapters::ChapterList;
using kinema::ui::player::ChaptersModel;

class TestChaptersModel : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void resetExposesEntries();
    void roleNamesAreStable();
};

void TestChaptersModel::resetExposesEntries()
{
    ChaptersModel m;
    ChapterList list;
    list << Chapter { 0.0,    QStringLiteral("Intro") }
         << Chapter { 90.0,   QStringLiteral("Chapter 1") }
         << Chapter { 1800.0, QStringLiteral("Outro") };
    m.reset(list);

    QCOMPARE(m.rowCount(), 3);
    QCOMPARE(m.count(), 3);

    const auto idx = m.index(1);
    QCOMPARE(m.data(idx, ChaptersModel::TimeRole).toDouble(), 90.0);
    QCOMPARE(m.data(idx, ChaptersModel::TitleRole).toString(),
        QStringLiteral("Chapter 1"));
}

void TestChaptersModel::roleNamesAreStable()
{
    ChaptersModel m;
    const auto names = m.roleNames();
    QCOMPARE(names.value(ChaptersModel::TimeRole),
        QByteArrayLiteral("time"));
    QCOMPARE(names.value(ChaptersModel::TitleRole),
        QByteArrayLiteral("title"));
}

QTEST_MAIN(TestChaptersModel)
#include "tst_player_chapters_model.moc"
