// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include <QDirIterator>
#include <QFile>
#include <QRegularExpression>
#include <QStringList>
#include <QTest>

class TstArchitectureBoundaries : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void includesFollowDependencyDirection();
};

namespace {

bool startsWithAny(const QString& value, const QStringList& prefixes)
{
    for (const auto& prefix : prefixes) {
        if (value.startsWith(prefix))
            return true;
    }
    return false;
}

} // namespace

void TstArchitectureBoundaries::includesFollowDependencyDirection()
{
    const QString sourceRoot = QStringLiteral(KINEMA_TEST_PROJECT_ROOT "/src/");
    const QRegularExpression includePattern(
        QStringLiteral("^\\s*#\\s*include\\s*\\\"([^\\\"]+)\\\""));
    QStringList violations;

    QDirIterator files(sourceRoot,
                       {QStringLiteral("*.h"), QStringLiteral("*.cpp")},
                       QDir::Files,
                       QDirIterator::Subdirectories);
    while (files.hasNext()) {
        const QString absolutePath = files.next();
        const QString relativePath = QDir(sourceRoot).relativeFilePath(absolutePath);
        QFile file(absolutePath);
        QVERIFY2(file.open(QIODevice::ReadOnly | QIODevice::Text), qPrintable(file.errorString()));

        int lineNumber = 0;
        while (!file.atEnd()) {
            ++lineNumber;
            const QString line = QString::fromUtf8(file.readLine());
            const auto match = includePattern.match(line);
            if (!match.hasMatch())
                continue;
            const QString dependency = match.captured(1);

            bool forbidden = false;
            if (relativePath.startsWith(QStringLiteral("domain/"))) {
                forbidden = startsWithAny(dependency,
                                          {QStringLiteral("api/"),
                                           QStringLiteral("app/"),
                                           QStringLiteral("config/"),
                                           QStringLiteral("controllers/"),
                                           QStringLiteral("playback/"),
                                           QStringLiteral("services/"),
                                           QStringLiteral("ui/")});
            } else if (relativePath.startsWith(QStringLiteral("playback/ports/"))) {
                forbidden = startsWithAny(dependency,
                                          {QStringLiteral("api/"),
                                           QStringLiteral("app/"),
                                           QStringLiteral("config/"),
                                           QStringLiteral("controllers/"),
                                           QStringLiteral("services/"),
                                           QStringLiteral("torrent/"),
                                           QStringLiteral("ui/")});
            } else if (relativePath.startsWith(QStringLiteral("playback/"))) {
                forbidden = startsWithAny(dependency,
                                          {QStringLiteral("controllers/"),
                                           QStringLiteral("services/"),
                                           QStringLiteral("ui/")});
            }

            if (forbidden) {
                violations.append(QStringLiteral("%1:%2 includes %3")
                                      .arg(relativePath)
                                      .arg(lineNumber)
                                      .arg(dependency));
            }
        }
    }

    QVERIFY2(violations.isEmpty(), qPrintable(violations.join(QLatin1Char('\n'))));
}

QTEST_GUILESS_MAIN(TstArchitectureBoundaries)
#include "tst_architecture_boundaries.moc"
