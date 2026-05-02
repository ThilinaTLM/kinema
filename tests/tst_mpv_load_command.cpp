// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/MpvLoadCommand.h"

#include <QTest>

using namespace kinema::ui::player::mpv_command;

class TestMpvLoadCommand : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void buildWithoutResume();
    void buildWithResume();
};

void TestMpvLoadCommand::buildWithoutResume()
{
    const auto args = buildLoadFileCommand(
        QUrl(QStringLiteral("https://example.com/video.mkv")));

    const QStringList expected {
        QStringLiteral("loadfile"),
        QStringLiteral("https://example.com/video.mkv"),
    };
    QCOMPARE(args, expected);
}

void TestMpvLoadCommand::buildWithResume()
{
    const auto args = buildLoadFileCommand(
        QUrl(QStringLiteral("https://example.com/video.mkv")),
        123.456);

    const QStringList expected {
        QStringLiteral("loadfile"),
        QStringLiteral("https://example.com/video.mkv"),
        QStringLiteral("replace"),
        QStringLiteral("start=123.456"),
    };
    QCOMPARE(args, expected);
    QVERIFY(!args.contains(QStringLiteral("-1")));
}

QTEST_GUILESS_MAIN(TestMpvLoadCommand)
#include "tst_mpv_load_command.moc"
