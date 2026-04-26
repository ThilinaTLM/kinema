// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/UrlRedactor.h"

#include <QtTest>

using kinema::core::redactUrlForLog;

class TestUrlRedactor : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void redactsTorrentioResolveToken();
    void redactsTorrentioConfigToken();
    void stripsUserInfoAndQuery();
    void keepsNormalUrlContext();
};

void TestUrlRedactor::redactsTorrentioResolveToken()
{
    const QUrl url(QStringLiteral("https://torrentio.strem.fun/resolve/realdebrid/SECRET/hash/null/0/file.mp4"));
    const QString redacted = redactUrlForLog(url);

    QVERIFY2(!redacted.contains(QStringLiteral("SECRET")), qPrintable(redacted));
    QVERIFY2(redacted.contains(QStringLiteral("/resolve/realdebrid/<redacted>/")),
        qPrintable(redacted));
    QVERIFY2(redacted.endsWith(QStringLiteral("/file.mp4")), qPrintable(redacted));
}

void TestUrlRedactor::redactsTorrentioConfigToken()
{
    const QUrl url(QStringLiteral("https://torrentio.strem.fun/sort=seeders|realdebrid=SECRET/stream/movie/tt123.json"));
    const QString redacted = redactUrlForLog(url);

    QVERIFY2(!redacted.contains(QStringLiteral("SECRET")), qPrintable(redacted));
    QVERIFY2(redacted.contains(QStringLiteral("realdebrid=<redacted>")),
        qPrintable(redacted));
    QVERIFY2(redacted.contains(QStringLiteral("/stream/movie/tt123.json")),
        qPrintable(redacted));
}

void TestUrlRedactor::stripsUserInfoAndQuery()
{
    const QUrl url(QStringLiteral("https://user:pass@example.test/path?api_key=SECRET&q=keep#frag"));
    const QString redacted = redactUrlForLog(url);

    QVERIFY2(!redacted.contains(QStringLiteral("user")), qPrintable(redacted));
    QVERIFY2(!redacted.contains(QStringLiteral("pass")), qPrintable(redacted));
    QVERIFY2(!redacted.contains(QStringLiteral("SECRET")), qPrintable(redacted));
    QVERIFY2(!redacted.contains(QStringLiteral("api_key")), qPrintable(redacted));
    QVERIFY2(!redacted.contains(QStringLiteral("frag")), qPrintable(redacted));
    QCOMPARE(redacted, QStringLiteral("https://example.test/path"));
}

void TestUrlRedactor::keepsNormalUrlContext()
{
    const QUrl url(QStringLiteral("https://example.test/api/v1/title.json?debug=true"));
    QCOMPARE(redactUrlForLog(url), QStringLiteral("https://example.test/api/v1/title.json"));
}

QTEST_MAIN(TestUrlRedactor)
#include "tst_url_redactor.moc"
