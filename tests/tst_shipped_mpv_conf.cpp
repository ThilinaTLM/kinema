// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include <QFile>
#include <QString>
#include <QStringList>
#include <QTest>

/// Read-back tests for the shipped mpv configuration.
///
/// The embedded player reuses one libmpv core for every queue item;
/// some mpv settings interact badly with that lifecycle (most
/// notably `idle=once`, which makes the core shut down after the
/// first file finishes and silently swallow every subsequent
/// `loadfile`). Pin those invariants here so a future config edit
/// can't quietly resurrect the "second queue item never loads"
/// regression.
class TestShippedMpvConf : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void initTestCase();
    void coreStaysIdleBetweenItems();

private:
    QStringList m_settingLines;
};

namespace {

QString shippedMpvConfPath()
{
    return QStringLiteral(KINEMA_TEST_PROJECT_ROOT
        "/data/kinema/mpv/mpv.conf");
}

QStringList loadEffectiveSettings(const QString& path)
{
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly | QIODevice::Text)) {
        return {};
    }
    QStringList out;
    const auto raw
        = QString::fromUtf8(f.readAll()).split(QLatin1Char('\n'));
    for (const auto& rawLine : raw) {
        const auto line = rawLine.trimmed();
        if (line.isEmpty() || line.startsWith(QLatin1Char('#'))) {
            continue;
        }
        out.append(line);
    }
    return out;
}

} // namespace

void TestShippedMpvConf::initTestCase()
{
    const auto path = shippedMpvConfPath();
    QVERIFY2(QFile::exists(path),
        qPrintable(QStringLiteral(
            "Shipped mpv.conf not found at %1").arg(path)));
    m_settingLines = loadEffectiveSettings(path);
    QVERIFY(!m_settingLines.isEmpty());
}

void TestShippedMpvConf::coreStaysIdleBetweenItems()
{
    // `idle=once` makes mpv tear its playback engine down after the
    // first file ends. With a long-lived embedded host, that strands
    // every queue item past the first on a black loading screen
    // (no `file-loaded`, no `end-file`). Only `idle=yes` is correct
    // for our reuse pattern.
    const QString badSetting = QStringLiteral("idle=once");
    QVERIFY2(!m_settingLines.contains(badSetting),
        "Shipped mpv.conf contains `idle=once` which breaks queue "
        "auto-advance \u2014 use `idle=yes` so the libmpv core stays "
        "alive across queue items.");
    QVERIFY2(m_settingLines.contains(QStringLiteral("idle=yes")),
        "Shipped mpv.conf must set `idle=yes` so the reused libmpv "
        "core accepts a fresh `loadfile` for each queue item.");
}

QTEST_GUILESS_MAIN(TestShippedMpvConf)
#include "tst_shipped_mpv_conf.moc"
