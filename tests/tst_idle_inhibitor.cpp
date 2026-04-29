// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/IdleInhibitor.h"

#include <QtTest>

#include <utility>

struct BackendStats {
    int inhibitCalls = 0;
    int uninhibitCalls = 0;
    bool active = false;
};

class FakeBackend final : public kinema::core::IdleInhibitor::Backend
{
public:
    FakeBackend(QString backendName, bool shouldSucceed, BackendStats& stats)
        : m_name(std::move(backendName))
        , m_shouldSucceed(shouldSucceed)
        , m_stats(stats)
    {
    }

    QString name() const override { return m_name; }

    bool inhibit(const QString&) override
    {
        ++m_stats.inhibitCalls;
        m_stats.active = m_shouldSucceed;
        return m_shouldSucceed;
    }

    void uninhibit() override
    {
        ++m_stats.uninhibitCalls;
        m_stats.active = false;
    }

private:
    QString m_name;
    bool m_shouldSucceed = true;
    BackendStats& m_stats;
};

class TestIdleInhibitor : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void deduplicatesActivate();
    void fallsBackToSecondary();
    void destructorReleases();
};

void TestIdleInhibitor::deduplicatesActivate()
{
    BackendStats primaryStats;
    BackendStats fallbackStats;
    auto* primary = new FakeBackend(
        QStringLiteral("primary"), true, primaryStats);
    auto* fallback = new FakeBackend(
        QStringLiteral("fallback"), true, fallbackStats);
    kinema::core::IdleInhibitor inhibitor {
        std::unique_ptr<kinema::core::IdleInhibitor::Backend>(primary),
        std::unique_ptr<kinema::core::IdleInhibitor::Backend>(fallback) };

    inhibitor.setActive(true, QStringLiteral("Playing a movie"));
    inhibitor.setActive(true, QStringLiteral("Playing a movie"));
    QCOMPARE(primaryStats.inhibitCalls, 1);
    QCOMPARE(fallbackStats.inhibitCalls, 0);

    inhibitor.setActive(false, QString());
    QCOMPARE(primaryStats.uninhibitCalls, 1);
    inhibitor.setActive(false, QString());
    QCOMPARE(primaryStats.uninhibitCalls, 1);
}

void TestIdleInhibitor::fallsBackToSecondary()
{
    BackendStats primaryStats;
    BackendStats fallbackStats;
    auto* primary = new FakeBackend(
        QStringLiteral("primary"), false, primaryStats);
    auto* fallback = new FakeBackend(
        QStringLiteral("fallback"), true, fallbackStats);
    kinema::core::IdleInhibitor inhibitor {
        std::unique_ptr<kinema::core::IdleInhibitor::Backend>(primary),
        std::unique_ptr<kinema::core::IdleInhibitor::Backend>(fallback) };

    inhibitor.setActive(true, QStringLiteral("Playing a movie"));
    QCOMPARE(primaryStats.inhibitCalls, 1);
    QCOMPARE(fallbackStats.inhibitCalls, 1);
    QVERIFY(inhibitor.isActive());

    inhibitor.setActive(false, QString());
    QCOMPARE(fallbackStats.uninhibitCalls, 1);
}

void TestIdleInhibitor::destructorReleases()
{
    BackendStats primaryStats;
    BackendStats fallbackStats;
    auto* primary = new FakeBackend(
        QStringLiteral("primary"), true, primaryStats);
    auto* fallback = new FakeBackend(
        QStringLiteral("fallback"), true, fallbackStats);
    {
        kinema::core::IdleInhibitor inhibitor {
            std::unique_ptr<kinema::core::IdleInhibitor::Backend>(primary),
            std::unique_ptr<kinema::core::IdleInhibitor::Backend>(fallback) };
        inhibitor.setActive(true, QStringLiteral("Playing a movie"));
        QCOMPARE(primaryStats.uninhibitCalls, 0);
    }
    QCOMPARE(primaryStats.uninhibitCalls, 1);
}

QTEST_GUILESS_MAIN(TestIdleInhibitor)
#include "tst_idle_inhibitor.moc"
