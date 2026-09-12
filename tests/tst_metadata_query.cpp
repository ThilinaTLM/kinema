// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/common/MetadataQuery.h"

#include <QTest>

using namespace kinema::api;

class TstMetadataQuery : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void bareLowercaseId()
    {
        const auto id = metadata_query::extractImdbTitleId(QStringLiteral("tt15145764"));
        QVERIFY(id.has_value());
        QCOMPARE(*id, QStringLiteral("tt15145764"));
    }

    void bareUppercaseIdNormalizes()
    {
        const auto id = metadata_query::extractImdbTitleId(QStringLiteral("TT15145764"));
        QVERIFY(id.has_value());
        QCOMPARE(*id, QStringLiteral("tt15145764"));
    }

    void desktopTitleUrl()
    {
        const auto id = metadata_query::extractImdbTitleId(
            QStringLiteral("https://www.imdb.com/title/tt15145764/"));
        QVERIFY(id.has_value());
        QCOMPARE(*id, QStringLiteral("tt15145764"));
    }

    void mobileTitleUrlWithQuery()
    {
        const auto id = metadata_query::extractImdbTitleId(
            QStringLiteral("https://m.imdb.com/title/tt15145764/?ref_=fn_al_tt_1#frag"));
        QVERIFY(id.has_value());
        QCOMPARE(*id, QStringLiteral("tt15145764"));
    }

    void nonTitleImdbUrlReturnsEmpty()
    {
        const auto id = metadata_query::extractImdbTitleId(
            QStringLiteral("https://www.imdb.com/name/nm0000001/"));
        QVERIFY(!id.has_value());
    }

    void ordinaryTitleContainingIdReturnsEmpty()
    {
        const auto id =
            metadata_query::extractImdbTitleId(QStringLiteral("movie tt15145764 freddy"));
        QVERIFY(!id.has_value());
    }
};

QTEST_MAIN(TstMetadataQuery)
#include "tst_metadata_query.moc"
