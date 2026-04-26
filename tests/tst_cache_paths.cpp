// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/CachePaths.h"

#include <QDir>
#include <QFile>
#include <QStandardPaths>
#include <QTemporaryFile>
#include <QTest>

using namespace kinema::core::cache;

class TstCachePaths : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void testRootMatchesCacheLocation()
    {
        const auto expected = QStandardPaths::writableLocation(
            QStandardPaths::CacheLocation);
        QCOMPARE(root().absolutePath(), QDir(expected).absolutePath());
    }

    void testSubtitlesDirIsBelowRoot()
    {
        const auto subs = subtitlesDir();
        QVERIFY(subs.exists());
        QVERIFY(subs.absolutePath().endsWith(QStringLiteral("/subtitles")));
        QVERIFY(subs.absolutePath().startsWith(root().absolutePath()));
    }

    void testDirSizeBytes()
    {
        QDir d = subtitlesDir();
        const auto path = d.absoluteFilePath(QStringLiteral("test_size_marker.bin"));
        QFile f(path);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.write(QByteArray(1234, 'x'));
        f.close();

        const qint64 sz = dirSizeBytes(d);
        QVERIFY(sz >= 1234);

        QFile::remove(path);
    }
};

QTEST_MAIN(TstCachePaths)
#include "tst_cache_paths.moc"
