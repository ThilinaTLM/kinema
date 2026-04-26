// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/SubtitleResultsModel.h"

#include "api/Subtitle.h"

#include <QSet>
#include <QSignalSpy>
#include <QtTest>

using kinema::api::SubtitleHit;
using kinema::ui::widgets::SubtitleResultsModel;

namespace {

QList<SubtitleHit> sampleHits()
{
    SubtitleHit a;
    a.fileId = QStringLiteral("100");
    a.language = QStringLiteral("eng");
    a.languageName = QStringLiteral("English");
    a.releaseName = QStringLiteral("Inception.2010.1080p.BluRay.x264-SPARKS");
    a.fileName = QStringLiteral("Inception.2010.eng.srt");
    a.format = QStringLiteral("srt");
    a.downloadCount = 24531;
    a.rating = 8.4;
    a.moviehashMatch = true;

    SubtitleHit b;
    b.fileId = QStringLiteral("200");
    b.language = QStringLiteral("spa");
    b.languageName = QStringLiteral("Spanish");
    b.releaseName = QStringLiteral("Inception.2010.1080p.BluRay.x264-CtrlHD");
    b.fileName = QStringLiteral("Inception.2010.spa.srt");
    b.format = QStringLiteral("srt");
    b.downloadCount = 9302;
    b.rating = 7.9;
    b.hearingImpaired = true;

    return { a, b };
}

} // namespace

class TestSubtitleResultsModel : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void emptyByDefault();
    void exposesRolesPerHit();
    void cachedFlagFlipsFromSet();
    void activeFlagFlipsFromSet();
    void displayRoleFallsBackToFileName();
    void roleNamesContainKnownKeys();
};

void TestSubtitleResultsModel::emptyByDefault()
{
    SubtitleResultsModel m;
    QCOMPARE(m.rowCount(), 0);
    QCOMPARE(m.count(), 0);
    QCOMPARE(m.data(m.index(0)), QVariant {});
}

void TestSubtitleResultsModel::exposesRolesPerHit()
{
    SubtitleResultsModel m;
    m.setHits(sampleHits());

    QCOMPARE(m.rowCount(), 2);
    QCOMPARE(m.data(m.index(0), SubtitleResultsModel::FileIdRole).toString(),
        QStringLiteral("100"));
    QCOMPARE(m.data(m.index(0), SubtitleResultsModel::LanguageRole).toString(),
        QStringLiteral("eng"));
    QCOMPARE(m.data(m.index(0),
                 SubtitleResultsModel::MoviehashMatchRole).toBool(),
        true);
    QCOMPARE(m.data(m.index(0), SubtitleResultsModel::DownloadCountRole).toInt(),
        24531);
    QCOMPARE(m.data(m.index(1), SubtitleResultsModel::HearingImpairedRole).toBool(),
        true);
    QCOMPARE(m.data(m.index(1), SubtitleResultsModel::CachedRole).toBool(),
        false);
    QCOMPARE(m.data(m.index(1), SubtitleResultsModel::ActiveRole).toBool(),
        false);
}

void TestSubtitleResultsModel::cachedFlagFlipsFromSet()
{
    SubtitleResultsModel m;
    m.setHits(sampleHits());

    QSignalSpy spy(&m, &QAbstractItemModel::dataChanged);
    m.setCachedFileIds({ QStringLiteral("200") });

    QCOMPARE(m.data(m.index(0), SubtitleResultsModel::CachedRole).toBool(),
        false);
    QCOMPARE(m.data(m.index(1), SubtitleResultsModel::CachedRole).toBool(),
        true);
    QVERIFY(spy.size() >= 1);

    // Idempotent re-set: no further dataChanged.
    const int before = spy.size();
    m.setCachedFileIds({ QStringLiteral("200") });
    QCOMPARE(spy.size(), before);
}

void TestSubtitleResultsModel::activeFlagFlipsFromSet()
{
    SubtitleResultsModel m;
    m.setHits(sampleHits());
    m.setActiveFileIds({ QStringLiteral("100") });

    QCOMPARE(m.data(m.index(0), SubtitleResultsModel::ActiveRole).toBool(),
        true);
    QCOMPARE(m.data(m.index(1), SubtitleResultsModel::ActiveRole).toBool(),
        false);
}

void TestSubtitleResultsModel::displayRoleFallsBackToFileName()
{
    SubtitleHit hit;
    hit.fileId = QStringLiteral("1");
    hit.fileName = QStringLiteral("nameless.srt");
    hit.language = QStringLiteral("eng");

    SubtitleResultsModel m;
    m.setHits({ hit });
    QCOMPARE(m.data(m.index(0), Qt::DisplayRole).toString(),
        QStringLiteral("nameless.srt"));
}

void TestSubtitleResultsModel::roleNamesContainKnownKeys()
{
    SubtitleResultsModel m;
    const auto names = m.roleNames();
    QVERIFY(names.contains(SubtitleResultsModel::FileIdRole));
    QVERIFY(names.contains(SubtitleResultsModel::CachedRole));
    QVERIFY(names.contains(SubtitleResultsModel::ActiveRole));
    QCOMPARE(names.value(SubtitleResultsModel::FileIdRole),
        QByteArray("fileId"));
}

QTEST_MAIN(TestSubtitleResultsModel)
#include "tst_subtitle_results_model.moc"
