// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "core/Player.h"

#include <QTest>
#include <QUrl>

using namespace kinema::core::player;

class TestPlayer : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void toFromString_data()
    {
        QTest::addColumn<Kind>("kind");
        QTest::addColumn<QString>("id");
        QTest::newRow("mpv") << Kind::Mpv << QStringLiteral("mpv");
        QTest::newRow("vlc") << Kind::Vlc << QStringLiteral("vlc");
        QTest::newRow("custom") << Kind::Custom << QStringLiteral("custom");
    }
    void toFromString()
    {
        QFETCH(Kind, kind);
        QFETCH(QString, id);
        QCOMPARE(::kinema::core::player::toString(kind), id);
        QVERIFY(fromString(id).has_value());
        QVERIFY(fromString(id).value() == kind);
        // Case-insensitive parse.
        QVERIFY(fromString(id.toUpper()).value() == kind);
    }

    void fromString_unknown()
    {
        QVERIFY(!fromString(QStringLiteral("quicktime")).has_value());
        QVERIFY(!fromString(QStringLiteral("")).has_value());
    }

    void mpvInvocation()
    {
        const QUrl url(QStringLiteral("https://cdn.real-debrid.com/foo.mkv"));
        const auto inv = buildInvocation(Kind::Mpv, url);
        QVERIFY(inv.isValid());
        QCOMPARE(inv.program, QStringLiteral("mpv"));
        // URL is the final argument, preceded by `--` so option parsing
        // can't reinterpret it as a flag.
        QVERIFY(inv.args.contains(QStringLiteral("--")));
        QCOMPARE(inv.args.last(), url.toString());
        // `--` must come immediately before the URL.
        QCOMPARE(inv.args.at(inv.args.size() - 2), QStringLiteral("--"));
        // A window hint keeps mpv from silently buffering in the
        // background with no UI feedback.
        QVERIFY(inv.args.contains(QStringLiteral("--force-window=immediate")));
    }

    void vlcInvocation()
    {
        const QUrl url(QStringLiteral("https://cdn.real-debrid.com/bar.mp4"));
        const auto inv = buildInvocation(Kind::Vlc, url);
        QVERIFY(inv.isValid());
        QCOMPARE(inv.program, QStringLiteral("vlc"));
        QCOMPARE(inv.args.last(), url.toString());
        QVERIFY(inv.args.contains(QStringLiteral("--play-and-exit")));
        QVERIFY(inv.args.contains(QStringLiteral("--no-one-instance")));
    }

    void customInvocation_appendsUrl()
    {
        const QUrl url(QStringLiteral("https://example.com/stream"));
        const auto inv = buildInvocation(Kind::Custom, url,
            QStringLiteral("/opt/bin/myplayer --quiet"));
        QVERIFY(inv.isValid());
        QCOMPARE(inv.program, QStringLiteral("/opt/bin/myplayer"));
        const QStringList expected {
            QStringLiteral("--quiet"),
            url.toString(),
        };
        QCOMPARE(inv.args, expected);
    }

    void customInvocation_substitutesPlaceholder()
    {
        const QUrl url(QStringLiteral("https://example.com/s.mkv"));
        const auto inv = buildInvocation(Kind::Custom, url,
            QStringLiteral("myplayer --url={url} --quiet"));
        QVERIFY(inv.isValid());
        QCOMPARE(inv.program, QStringLiteral("myplayer"));
        const QStringList expected {
            QStringLiteral("--url=") + url.toString(),
            QStringLiteral("--quiet"),
        };
        QCOMPARE(inv.args, expected);
    }

    void customInvocation_quoting()
    {
        const QUrl url(QStringLiteral("https://example.com/s.mkv"));
        const auto inv = buildInvocation(Kind::Custom, url,
            QStringLiteral("\"/opt/My Player/bin/player\" --title 'Hello World'"));
        QVERIFY(inv.isValid());
        QCOMPARE(inv.program, QStringLiteral("/opt/My Player/bin/player"));
        QCOMPARE(inv.args.first(), QStringLiteral("--title"));
        QCOMPARE(inv.args.at(1), QStringLiteral("Hello World"));
        QCOMPARE(inv.args.last(), url.toString());
    }

    void customInvocation_emptyIsInvalid()
    {
        const QUrl url(QStringLiteral("https://example.com/s.mkv"));
        const auto inv = buildInvocation(Kind::Custom, url, QString {});
        QVERIFY(!inv.isValid());
    }
};

QTEST_GUILESS_MAIN(TestPlayer)
#include "tst_player.moc"
