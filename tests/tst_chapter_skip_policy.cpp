// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/ChapterSkipPolicy.h"

#include <QTest>

using namespace kinema;
using namespace kinema::core::chapters;
using namespace kinema::playback::policy;

namespace {

Chapter mk(double t, const QString& title = {})
{
    Chapter c;
    c.time = t;
    c.title = title;
    return c;
}

} // namespace

class TstChapterSkipPolicy : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void classification()
    {
        QCOMPARE(classifyChapterTitle(QStringLiteral("intro")),
            SkipKind::Intro);
        QCOMPARE(classifyChapterTitle(QStringLiteral("Opening Theme")),
            SkipKind::Intro);
        QCOMPARE(classifyChapterTitle(QStringLiteral("Credits")),
            SkipKind::Credits);
        QCOMPARE(classifyChapterTitle(QStringLiteral("End Credits")),
            SkipKind::Credits);
        QCOMPARE(classifyChapterTitle(QStringLiteral("Outro")),
            SkipKind::Outro);
        QCOMPARE(classifyChapterTitle(QStringLiteral("Ending")),
            SkipKind::Outro);
    }

    void activeReturnsCurrentSkip()
    {
        ChapterList chapters {
            mk(0.0, QStringLiteral("Intro")),
            mk(90.0, QStringLiteral("Episode")),
            mk(1300.0, QStringLiteral("End Credits")),
        };
        const auto s = activeSkipChapter(chapters, 30.0, 1400.0);
        QVERIFY(s.has_value());
        QCOMPARE(s->kind, SkipKind::Intro);
        QCOMPARE(s->startSec, 0.0);
        QCOMPARE(s->endSec, 90.0);
    }

    void inactiveReturnsNullopt()
    {
        ChapterList chapters {
            mk(0.0, QStringLiteral("Intro")),
            mk(90.0, QStringLiteral("Episode")),
        };
        QVERIFY(!activeSkipChapter(chapters, 200.0, 1400.0).has_value());
    }

    void buttonLabelLocalised()
    {
        QVERIFY(!skipButtonLabel(SkipKind::Credits).isEmpty());
        QVERIFY(!skipButtonLabel(SkipKind::Outro).isEmpty());
        QVERIFY(!skipButtonLabel(SkipKind::Intro).isEmpty());
    }
};

QTEST_MAIN(TstChapterSkipPolicy)
#include "tst_chapter_skip_policy.moc"
