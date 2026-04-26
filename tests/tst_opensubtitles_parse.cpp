// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/OpenSubtitlesParse.h"

#include <QFile>
#include <QJsonDocument>
#include <QString>
#include <QTest>

#ifndef KINEMA_TEST_FIXTURES_DIR
#error KINEMA_TEST_FIXTURES_DIR not defined; check tests/CMakeLists.txt
#endif

using namespace kinema::api;

namespace {

QJsonDocument loadFixture(const char* name)
{
    QFile f(QString::fromLatin1(KINEMA_TEST_FIXTURES_DIR) + QLatin1Char('/')
        + QString::fromLatin1("opensubtitles/") + QString::fromLatin1(name));
    if (!f.open(QIODevice::ReadOnly)) {
        return {};
    }
    return QJsonDocument::fromJson(f.readAll());
}

} // namespace

class TstOpenSubtitlesParse : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testSearchMovie()
    {
        const auto doc = loadFixture("search-movie.json");
        QVERIFY(!doc.isNull());

        const auto hits = opensubtitles::parseSearch(doc);
        QCOMPARE(hits.size(), 2);

        const auto& h1 = hits.at(0);
        QCOMPARE(h1.fileId, QStringLiteral("555111"));
        QCOMPARE(h1.language, QStringLiteral("eng"));
        QCOMPARE(h1.languageName, QStringLiteral("English"));
        QCOMPARE(h1.downloadCount, 1234);
        QCOMPARE(h1.rating, 9.2);
        QVERIFY(h1.moviehashMatch);
        QVERIFY(!h1.hearingImpaired);
        QCOMPARE(h1.fps, 23);
        QCOMPARE(h1.uploader, QStringLiteral("alice"));
        QCOMPARE(h1.format, QStringLiteral("srt"));
        QCOMPARE(h1.fileName, QStringLiteral("the.matrix.1999.1080p.eng.srt"));

        const auto& h2 = hits.at(1);
        QCOMPARE(h2.fileId, QStringLiteral("555222"));
        QCOMPARE(h2.language, QStringLiteral("spa"));
        QCOMPARE(h2.languageName, QStringLiteral("Spanish"));
        QVERIFY(h2.hearingImpaired);
        QVERIFY(!h2.moviehashMatch);
    }

    void testSearchSeries()
    {
        const auto doc = loadFixture("search-series.json");
        QVERIFY(!doc.isNull());
        const auto hits = opensubtitles::parseSearch(doc);
        QCOMPARE(hits.size(), 1);
        QCOMPARE(hits.first().fileId, QStringLiteral("777333"));
        QCOMPARE(hits.first().releaseName,
            QStringLiteral("Game.of.Thrones.S01E02.The.Kingsroad.1080p"));
    }

    void testDownloadOk()
    {
        const auto doc = loadFixture("download-ok.json");
        QVERIFY(!doc.isNull());
        const auto t = opensubtitles::parseDownload(doc);
        QVERIFY(t.link.isValid());
        QCOMPARE(t.fileName, QStringLiteral("the.matrix.1999.1080p.eng.srt"));
        QCOMPARE(t.remaining, 19);
        QCOMPARE(t.format, QStringLiteral("srt"));
        QVERIFY(t.resetAt.isValid());
    }

    void testDownloadQuotaEmptyLink()
    {
        const auto doc = loadFixture("download-quota.json");
        QVERIFY(!doc.isNull());
        const auto t = opensubtitles::parseDownload(doc);
        QVERIFY(t.link.isEmpty() || !t.link.isValid()
            || t.link.toString().isEmpty());
        QCOMPARE(t.remaining, 0);
        QVERIFY(t.resetAt.isValid());
    }

    void testLoginOk()
    {
        const auto doc = loadFixture("login-ok.json");
        QVERIFY(!doc.isNull());
        const auto token = opensubtitles::parseLogin(doc);
        QVERIFY(token.startsWith(QStringLiteral("eyJ")));
    }

    void testInvalidShapesThrow()
    {
        const auto bad = QJsonDocument::fromJson(QByteArrayLiteral("[1,2,3]"));
        QVERIFY_EXCEPTION_THROWN(opensubtitles::parseSearch(bad),
            std::runtime_error);
        QVERIFY_EXCEPTION_THROWN(opensubtitles::parseDownload(bad),
            std::runtime_error);
        QVERIFY_EXCEPTION_THROWN(opensubtitles::parseLogin(bad),
            std::runtime_error);
    }

    void testLoginMissingTokenThrows()
    {
        const auto doc = QJsonDocument::fromJson(QByteArrayLiteral(
            "{\"status\": 200}"));
        QVERIFY_EXCEPTION_THROWN(opensubtitles::parseLogin(doc),
            std::runtime_error);
    }
};

QTEST_MAIN(TstOpenSubtitlesParse)
#include "tst_opensubtitles_parse.moc"
