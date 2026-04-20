// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "core/StreamFilter.h"

#include <QTest>

using namespace kinema;
using namespace kinema::core::stream_filter;

namespace {

api::Stream makeStream(const QString& releaseName,
    bool rdCached = false,
    const QString& detailsText = {})
{
    api::Stream s;
    s.releaseName = releaseName;
    s.detailsText = detailsText;
    s.rdCached = rdCached;
    return s;
}

} // namespace

class TstStreamFilter : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void emptyFilters_passThrough()
    {
        const QList<api::Stream> in { makeStream(QStringLiteral("A")),
            makeStream(QStringLiteral("B")) };
        const auto out = apply(in, {});
        QCOMPARE(out.size(), 2);
        QCOMPARE(out[0].releaseName, QStringLiteral("A"));
        QCOMPARE(out[1].releaseName, QStringLiteral("B"));
    }

    void cachedOnly_keepsOnlyRDCached()
    {
        const QList<api::Stream> in {
            makeStream(QStringLiteral("one"), /*rdCached=*/false),
            makeStream(QStringLiteral("two"), /*rdCached=*/true),
            makeStream(QStringLiteral("three"), /*rdCached=*/false),
            makeStream(QStringLiteral("four"), /*rdCached=*/true),
        };
        ClientFilters f;
        f.cachedOnly = true;
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 2);
        QCOMPARE(out[0].releaseName, QStringLiteral("two"));
        QCOMPARE(out[1].releaseName, QStringLiteral("four"));
    }

    void cachedOnly_off_keepsAll()
    {
        const QList<api::Stream> in {
            makeStream(QStringLiteral("a"), /*rdCached=*/false),
            makeStream(QStringLiteral("b"), /*rdCached=*/true),
        };
        ClientFilters f;
        f.cachedOnly = false;
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 2);
    }

    void blocklist_matchesReleaseNameCaseInsensitively()
    {
        const QList<api::Stream> in {
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
        const QList<api::Stream> in {
            makeStream(QStringLiteral("Some.Release.1080p"),
                false, QStringLiteral("Hindi dub · 1.2 GB")),
            makeStream(QStringLiteral("Other.Release.1080p"),
                false, QStringLiteral("English · 1.3 GB")),
        };
        ClientFilters f;
        f.keywordBlocklist = { QStringLiteral("Hindi") };
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 1);
        QCOMPARE(out[0].releaseName, QStringLiteral("Other.Release.1080p"));
    }

    void blocklist_emptyKeywordsSkipped()
    {
        const QList<api::Stream> in { makeStream(QStringLiteral("hello")) };
        ClientFilters f;
        f.keywordBlocklist = { QString {}, QStringLiteral("  ") };
        // Note: whitespace-only keywords are NOT special-cased — they
        // match anything that contains whitespace. The caller is
        // expected to have trimmed input. We do verify empty QStrings
        // are ignored (they'd otherwise match every stream).
        const auto outEmpty = apply(in, { false, { QString {} } });
        QCOMPARE(outEmpty.size(), 1);
    }

    void blocklist_emptyMeansPassThrough()
    {
        const QList<api::Stream> in {
            makeStream(QStringLiteral("anything")),
            makeStream(QStringLiteral("else")),
        };
        ClientFilters f;
        f.keywordBlocklist = {};
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 2);
    }

    void combined_cachedAndBlocklist()
    {
        const QList<api::Stream> in {
            makeStream(QStringLiteral("Foo.CAM"), /*rdCached=*/true),
            makeStream(QStringLiteral("Foo.1080p"), /*rdCached=*/true),
            makeStream(QStringLiteral("Foo.1080p.uncached"), /*rdCached=*/false),
        };
        ClientFilters f;
        f.cachedOnly = true;
        f.keywordBlocklist = { QStringLiteral("cam") };
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 1);
        QCOMPARE(out[0].releaseName, QStringLiteral("Foo.1080p"));
    }

    void matchesBlocklist_helper()
    {
        const auto s = makeStream(QStringLiteral("Release.HDR10.2160p"));
        QVERIFY(matchesBlocklist(s, { QStringLiteral("HDR") }));
        QVERIFY(matchesBlocklist(s, { QStringLiteral("hdr10") }));
        QVERIFY(!matchesBlocklist(s, { QStringLiteral("dolby") }));
        QVERIFY(!matchesBlocklist(s, {}));
    }

    void orderIsPreserved()
    {
        const QList<api::Stream> in {
            makeStream(QStringLiteral("aaa"), true),
            makeStream(QStringLiteral("bbb"), false),
            makeStream(QStringLiteral("ccc"), true),
            makeStream(QStringLiteral("ddd"), true),
        };
        ClientFilters f;
        f.cachedOnly = true;
        const auto out = apply(in, f);
        QCOMPARE(out.size(), 3);
        QCOMPARE(out[0].releaseName, QStringLiteral("aaa"));
        QCOMPARE(out[1].releaseName, QStringLiteral("ccc"));
        QCOMPARE(out[2].releaseName, QStringLiteral("ddd"));
    }
};

QTEST_APPLESS_MAIN(TstStreamFilter)
#include "tst_stream_filter.moc"
