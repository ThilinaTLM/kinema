// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/widgets/PlayerSeekBar.h"

#include <QMouseEvent>
#include <QSignalSpy>
#include <QTest>

using namespace kinema::ui::player::widgets;

class TestPlayerSeekBar : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        // Keep the bar generously wide so trackLeftPx / trackRightPx
        // give us enough span to exercise the edges without aliasing.
        m_bar = new PlayerSeekBar;
        m_bar->resize(400, 40);
        m_bar->setDuration(3600.0); // 1 hour
    }

    void cleanupTestCase()
    {
        delete m_bar;
        m_bar = nullptr;
    }

    // The track has fixed side margins; clicks outside the track
    // still clamp into [0, duration].
    void timestampForXClampsToBounds()
    {
        QCOMPARE(m_bar->timestampForX(-1000), 0.0);
        QCOMPARE(m_bar->timestampForX(10'000),
            m_bar->duration());
    }

    // Midpoint of the track ~= midpoint of the timeline.
    void timestampForXMidpoint()
    {
        const int mid = (m_bar->trackLeftPx() + m_bar->trackRightPx()) / 2;
        const double t = m_bar->timestampForX(mid);
        const double mid_s = m_bar->duration() / 2.0;
        // Allow one-pixel of rounding slack either way.
        const double tolerance = m_bar->duration()
            / (m_bar->trackRightPx() - m_bar->trackLeftPx());
        QVERIFY2(std::abs(t - mid_s) <= tolerance,
            qPrintable(QStringLiteral(
                "midpoint timestamp off: got %1 expected ~%2")
                    .arg(t)
                    .arg(mid_s)));
    }

    // No duration → all clicks collapse to 0.
    void zeroDurationIsInert()
    {
        PlayerSeekBar idle;
        idle.resize(300, 30);
        // No duration set (default 0).
        QCOMPARE(idle.timestampForX(100), 0.0);

        // A click should NOT emit seekRequested when duration is 0.
        QSignalSpy spy(&idle, &PlayerSeekBar::seekRequested);
        QMouseEvent press(QEvent::MouseButtonPress, QPointF(100, 15),
            QPointF(100, 15), Qt::LeftButton, Qt::LeftButton,
            Qt::NoModifier);
        QApplication::sendEvent(&idle, &press);
        QMouseEvent release(QEvent::MouseButtonRelease, QPointF(100, 15),
            QPointF(100, 15), Qt::LeftButton, Qt::NoButton,
            Qt::NoModifier);
        QApplication::sendEvent(&idle, &release);
        QCOMPARE(spy.count(), 0);
    }

    // Click → drag → release emits one seekRequested with the
    // release timestamp.
    void clickAndReleaseEmitsSeek()
    {
        QSignalSpy spy(m_bar, &PlayerSeekBar::seekRequested);
        const int targetX = m_bar->trackLeftPx()
            + (m_bar->trackRightPx() - m_bar->trackLeftPx()) / 4;

        QMouseEvent press(QEvent::MouseButtonPress,
            QPointF(targetX, 20), QPointF(targetX, 20),
            Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
        QApplication::sendEvent(m_bar, &press);
        QMouseEvent release(QEvent::MouseButtonRelease,
            QPointF(targetX, 20), QPointF(targetX, 20),
            Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
        QApplication::sendEvent(m_bar, &release);

        QCOMPARE(spy.count(), 1);
        const double got = spy.first().first().toDouble();
        const double expected = m_bar->timestampForX(targetX);
        QVERIFY(std::abs(got - expected) < 0.01);
    }

    // Chapters that fall outside [0, duration] are silently ignored
    // (they can appear for malformed demuxers). This asserts the
    // setter doesn't reject the whole list.
    void chaptersAreStored()
    {
        m_bar->setChapters({ 0.0, 120.0, 4000.0 });
        QCOMPARE(m_bar->chapters().size(), 3);
    }

    // cacheAhead is clamped: negative and NaN values collapse to 0.
    void cacheAheadClamps()
    {
        m_bar->setCacheAhead(-5.0);
        QCOMPARE(m_bar->cacheAhead(), 0.0);
        m_bar->setCacheAhead(std::nan("1"));
        QCOMPARE(m_bar->cacheAhead(), 0.0);
        m_bar->setCacheAhead(30.0);
        QCOMPARE(m_bar->cacheAhead(), 30.0);
    }

private:
    PlayerSeekBar* m_bar {nullptr};
};

QTEST_MAIN(TestPlayerSeekBar)
#include "tst_player_seek_bar.moc"
