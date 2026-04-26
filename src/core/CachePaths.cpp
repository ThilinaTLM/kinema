// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/CachePaths.h"

#include <QDirIterator>
#include <QFileInfo>
#include <QStandardPaths>

namespace kinema::core::cache {

QDir root()
{
    const auto path = QStandardPaths::writableLocation(
        QStandardPaths::CacheLocation);
    QDir d(path);
    if (!d.exists()) {
        d.mkpath(QStringLiteral("."));
    }
    return d;
}

QDir subtitlesDir()
{
    QDir d = root();
    const auto subPath = d.absoluteFilePath(QStringLiteral("subtitles"));
    QDir s(subPath);
    if (!s.exists()) {
        s.mkpath(QStringLiteral("."));
    }
    return s;
}

qint64 dirSizeBytes(const QDir& dir)
{
    if (!dir.exists()) {
        return 0;
    }
    qint64 total = 0;
    QDirIterator it(dir.absolutePath(),
        QDir::Files | QDir::NoSymLinks | QDir::Hidden,
        QDirIterator::Subdirectories);
    while (it.hasNext()) {
        it.next();
        total += it.fileInfo().size();
    }
    return total;
}

} // namespace kinema::core::cache
