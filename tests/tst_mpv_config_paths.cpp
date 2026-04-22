// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MpvConfigPaths.h"

#include <QDir>
#include <QFile>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema::core;

class TestMpvConfigPaths : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_tmp = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmp->isValid());

        // Point GenericDataLocation at our tmpdir and wipe any
        // system-wide search path so the developer's real
        // ~/.local/share/kinema/mpv or /usr/share/kinema/mpv can't
        // leak into the "missing files" branch. Qt's QStandardPaths
        // on Linux honours these env vars directly, without needing
        // setTestModeEnabled (which would override them).
        //
        // XDG_DATA_DIRS is pointed at a non-existent subdir rather
        // than left empty — an empty value would reinstate Qt's
        // hard-coded defaults (/usr/local/share, /usr/share).
        qputenv("XDG_DATA_HOME", m_tmp->path().toUtf8());
        const QByteArray dummyDirs
            = (m_tmp->path() + QStringLiteral("/does-not-exist"))
                  .toUtf8();
        qputenv("XDG_DATA_DIRS", dummyDirs);
    }

    void cleanup()
    {
        qunsetenv("XDG_DATA_HOME");
        qunsetenv("XDG_DATA_DIRS");
        m_tmp.reset();
    }

    // Resolution succeeds when the files are present in the
    // XDG_DATA_HOME-rooted kinema/mpv directory.
    void resolvesFromGenericDataLocation()
    {
        const QString subdir = m_tmp->path()
            + QStringLiteral("/kinema/mpv");
        QVERIFY(QDir().mkpath(subdir));
        writeFile(subdir + QStringLiteral("/mpv.conf"),
            QByteArrayLiteral("# placeholder\n"));
        writeFile(subdir + QStringLiteral("/input.conf"),
            QByteArrayLiteral("# placeholder\n"));

        const auto dir = mpv_config::dataDir();
        QVERIFY(!dir.isEmpty());
        QCOMPARE(QDir(dir).canonicalPath(),
            QDir(subdir).canonicalPath());
        QVERIFY(QFile::exists(mpv_config::mpvConfPath()));
        QVERIFY(QFile::exists(mpv_config::inputConfPath()));
    }

    // When the directory exists but a specific file is missing, only
    // the present files resolve; missing ones return empty strings
    // without crashing.
    void partialPresence()
    {
        const QString subdir = m_tmp->path()
            + QStringLiteral("/kinema/mpv");
        QVERIFY(QDir().mkpath(subdir));
        writeFile(subdir + QStringLiteral("/mpv.conf"),
            QByteArrayLiteral("# placeholder\n"));
        // input.conf deliberately absent.

        QVERIFY(!mpv_config::mpvConfPath().isEmpty());
        QVERIFY(mpv_config::inputConfPath().isEmpty());
    }

    // dataDir() falls through to the build-tree fallback only when
    // GenericDataLocation cannot resolve the subdirectory. Here we
    // keep XDG empty and verify that either (a) the build-tree
    // fallback is populated (installed dev build, most common CI
    // case) or (b) dataDir() returns empty without throwing.
    //
    // The test doesn't assert a specific path because the build-tree
    // directory depends on where CMake put it; it asserts that the
    // call is a harmless no-op on missing state.
    void missingFilesReturnEmpty()
    {
        // No kinema/mpv under our tmpdir.
        const QString dir = mpv_config::dataDir();
        if (dir.isEmpty()) {
            QVERIFY(mpv_config::mpvConfPath().isEmpty());
            QVERIFY(mpv_config::inputConfPath().isEmpty());
            return;
        }
        // Fallback kicked in — paths must still be consistent.
        QVERIFY(QDir(dir).exists());
    }

private:
    void writeFile(const QString& path, const QByteArray& content)
    {
        QFile f(path);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.write(content);
    }

    std::unique_ptr<QTemporaryDir> m_tmp;
};

QTEST_GUILESS_MAIN(TestMpvConfigPaths)
#include "tst_mpv_config_paths.moc"
