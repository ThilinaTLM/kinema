// SPDX-License-Identifier: Apache-2.0
#include <QProcess>
#include <QTest>

class AppImagePackagingTest : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void packagingContracts()
    {
        QProcess process;
        process.setProcessChannelMode(QProcess::MergedChannels);
        process.start(
            QStringLiteral("python3"),
            {QStringLiteral(KINEMA_TEST_PROJECT_ROOT "/tests/test_appimage_packaging.py")});
        QVERIFY2(process.waitForStarted(), qPrintable(process.errorString()));
        QVERIFY2(process.waitForFinished(60000), qPrintable(process.errorString()));
        const auto output = process.readAll();
        QVERIFY2(process.exitStatus() == QProcess::NormalExit, output.constData());
        QVERIFY2(process.exitCode() == 0, output.constData());
    }
};
QTEST_GUILESS_MAIN(AppImagePackagingTest)
#include "tst_appimage_packaging.moc"
