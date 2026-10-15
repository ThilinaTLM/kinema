// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/util/StreamFilter.h"

#include <QTest>

using namespace kinema;
using namespace kinema::core::stream_filter;

namespace {

domain::Stream makeStream(const QString& releaseName,
    const QString& detailsText = {})
{
    domain::Stream s;
    s.releaseName = releaseName;
    s.detailsText = detailsText;
    return s;
}

} // namespace

class TstStreamFilter : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void emptyFilters_passThrough()
    {
        const QList<domain::Stream> in { makeStream(QStringLiteral("A")),
            makeStream(QStringLiteral("B")) };
        const auto out = apply(in, {});
        QCOMPARE(out.size(), 2);
        QCOMPARE(out[0].releaseName, QStringLiteral("A"));
        QCOMPARE(out[1].releaseName, QStringLiteral("B"));
    }

    void blocklist_matchesReleaseNameCaseInsensitively()
    {
        const QList<domain::Stream> in {
            makeStream(QStringLiteral("The.Matrix.1999.CAM.x264")),
            makeStream(QStringLiteral("The.Matrix.1999.1080p.BluRay")),
            makeStream(QStringLiteral("The.Matrix.1999.720p.HDCam")),
        };
        ClientFilters f;
        f.keywordBlocklist = { QStringLiteral("cam") };
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 1);
        QCOMPARE(out[0].releaseName, QStringLiteral("The.Matrix.1999.1080p.BluRay"));
    }

    void blocklist_matchesDetailsText()
    {
        const QList<domain::Stream> in {
            makeStream(QStringLiteral("Some.Release.1080p"),
                QStringLiteral("Hindi dub · 1.2 GB")),
            makeStream(QStringLiteral("Other.Release.1080p"),
                QStringLiteral("English · 1.3 GB")),
        };
        ClientFilters f;
        f.keywordBlocklist = { QStringLiteral("Hindi") };
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 1);
        QCOMPARE(out[0].releaseName, QStringLiteral("Other.Release.1080p"));
    }

    void blocklist_emptyKeywordsSkipped()
    {
        const QList<domain::Stream> in { makeStream(QStringLiteral("hello")) };
        ClientFilters f;
        f.keywordBlocklist = { QString {}, QStringLiteral("  ") };
        // Note: whitespace-only keywords are NOT special-cased — they
        // match anything that contains whitespace. The caller is
        // expected to have trimmed input. We do verify empty QStrings
        // are ignored (they'd otherwise match every stream).
        ClientFilters fEmpty;
        fEmpty.keywordBlocklist = { QString {} };
        const auto outEmpty = apply(in, fEmpty);
        QCOMPARE(outEmpty.size(), 1);
    }

    void blocklist_emptyMeansPassThrough()
    {
        const QList<domain::Stream> in {
            makeStream(QStringLiteral("anything")),
            makeStream(QStringLiteral("else")),
        };
        ClientFilters f;
        f.keywordBlocklist = {};
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 2);
    }

    void matchesBlocklist_helper()
    {
        const auto s = makeStream(QStringLiteral("Release.HDR10.2160p"));
        QVERIFY(matchesBlocklist(s, { QStringLiteral("HDR") }));
        QVERIFY(matchesBlocklist(s, { QStringLiteral("hdr10") }));
        QVERIFY(!matchesBlocklist(s, { QStringLiteral("dolby") }));
        QVERIFY(!matchesBlocklist(s, {}));
    }

    void excludedCategoriesNonen_dropsRowsWithNonEnglishLanguage()
    {
        domain::Stream en;
        en.releaseName = QStringLiteral("en.row");
        en.language = QStringLiteral("en");
        domain::Stream es;
        es.releaseName = QStringLiteral("es.row");
        es.language = QStringLiteral("es");
        domain::Stream unknown;
        unknown.releaseName = QStringLiteral("unknown.row");
        // empty language — pass through (Torrentio rows)

        ClientFilters f;
        f.excludedCategories = { QStringLiteral("nonen") };
        const auto out = apply({ en, es, unknown }, f);
        QCOMPARE(out.size(), 2);
        QCOMPARE(out[0].releaseName, QStringLiteral("en.row"));
        QCOMPARE(out[1].releaseName, QStringLiteral("unknown.row"));
    }

    void excludedCategoriesNonen_languageCompareIsCaseInsensitive()
    {
        domain::Stream upper;
        upper.releaseName = QStringLiteral("upper.en");
        upper.language = QStringLiteral("EN");
        domain::Stream mixed;
        mixed.releaseName = QStringLiteral("mixed.es");
        mixed.language = QStringLiteral("ES");

        ClientFilters f;
        f.excludedCategories = { QStringLiteral("nonen") };
        const auto out = apply({ upper, mixed }, f);
        QCOMPARE(out.size(), 1);
        QCOMPARE(out[0].releaseName, QStringLiteral("upper.en"));
    }

    void orderIsPreserved()
    {
        const QList<domain::Stream> in {
            makeStream(QStringLiteral("aaa")),
            makeStream(QStringLiteral("bbb.cam")),
            makeStream(QStringLiteral("ccc")),
            makeStream(QStringLiteral("ddd")),
        };
        ClientFilters f;
        f.keywordBlocklist = { QStringLiteral("cam") };
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 3);
        QCOMPARE(out[0].releaseName, QStringLiteral("aaa"));
        QCOMPARE(out[1].releaseName, QStringLiteral("ccc"));
        QCOMPARE(out[2].releaseName, QStringLiteral("ddd"));
    }
};

QTEST_APPLESS_MAIN(TstStreamFilter)
#include "tst_stream_filter.moc"
