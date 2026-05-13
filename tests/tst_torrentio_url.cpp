// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/util/TorrentioUrl.h"

#include <QTest>
#include <QUrl>

using namespace kinema::core::torrentio;

class TstTorrentioUrl : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void parsesTypicalAllDebridResolveUrl()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/alldebrid/"
            "kADNq6OBK8l7iVf3eQUe/"
            "255e2890a1a724bcdd93b1120dd08a5a8e11b949/"
            "The%20Rookie%20S02E01.mkv/20/"
            "The%20Rookie%20S02E01.mkv"));
        const auto info = parseResolveUrl(u);
        QVERIFY(info.has_value());
        QCOMPARE(info->infoHash,
            QStringLiteral("255e2890a1a724bcdd93b1120dd08a5a8e11b949"));
        QCOMPARE(info->fileIndex, 20);
        QCOMPARE(info->fileNameHint,
            QStringLiteral("The Rookie S02E01.mkv"));
    }

    void parsesRealDebridResolveUrl()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/realdebrid/"
            "ABCDEFGHIJKLMN/"
            "aabb1122ccdd3344eeff5566778899aabbccddee/"
            "Some.Movie.2024.1080p.mkv/0/"
            "Some.Movie.2024.1080p.mkv"));
        const auto info = parseResolveUrl(u);
        QVERIFY(info.has_value());
        QCOMPARE(info->infoHash,
            QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee"));
        QCOMPARE(info->fileIndex, 0);
        QCOMPARE(info->fileNameHint,
            QStringLiteral("Some.Movie.2024.1080p.mkv"));
    }

    void acceptsAlternateTorrentioMirrorHost()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.example.com/resolve/alldebrid/TOK/"
            "aabb1122ccdd3344eeff5566778899aabbccddee/x.mkv/3/x.mkv"));
        const auto info = parseResolveUrl(u);
        QVERIFY(info.has_value());
        QCOMPARE(info->fileIndex, 3);
        QCOMPARE(info->fileNameHint, QStringLiteral("x.mkv"));
    }

    void rejectsUnknownHost()
    {
        const QUrl u(QStringLiteral(
            "https://example.com/resolve/alldebrid/TOK/"
            "aabb1122ccdd3344eeff5566778899aabbccddee/x.mkv/0/x.mkv"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }

    void rejectsShortHash()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/alldebrid/TOK/"
            "aabb1122ccdd3344eeff5566778899aabbccdd/x.mkv/0/x.mkv"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }

    void rejectsNonHexHash()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/alldebrid/TOK/"
            "gabb1122ccdd3344eeff5566778899aabbccddee/x.mkv/0/x.mkv"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }

    void rejectsMissingSegments()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/alldebrid/TOK/"
            "aabb1122ccdd3344eeff5566778899aabbccddee"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }

    void rejectsNonIntegerFileIndex()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/alldebrid/TOK/"
            "aabb1122ccdd3344eeff5566778899aabbccddee/x.mkv/abc/x.mkv"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }

    void rejectsNegativeFileIndex()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/alldebrid/TOK/"
            "aabb1122ccdd3344eeff5566778899aabbccddee/x.mkv/-1/x.mkv"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }

    void normalisesUppercaseHash()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/resolve/alldebrid/TOK/"
            "AABB1122CCDD3344EEFF5566778899AABBCCDDEE/x.mkv/0/x.mkv"));
        const auto info = parseResolveUrl(u);
        QVERIFY(info.has_value());
        QCOMPARE(info->infoHash,
            QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee"));
    }

    void returnsNulloptForNonResolvePath()
    {
        const QUrl u(QStringLiteral(
            "https://torrentio.strem.fun/sort=seeders/stream/movie/"
            "tt0133093.json"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }

    void rejectsNonHttpScheme()
    {
        const QUrl u(QStringLiteral(
            "ftp://torrentio.strem.fun/resolve/alldebrid/TOK/"
            "aabb1122ccdd3344eeff5566778899aabbccddee/x.mkv/0/x.mkv"));
        QVERIFY(!parseResolveUrl(u).has_value());
    }
};

QTEST_APPLESS_MAIN(TstTorrentioUrl)
#include "tst_torrentio_url.moc"
