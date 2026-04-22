// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MpvConfigPaths.h"

#include <QDir>
#include <QFileInfo>
#include <QStandardPaths>

#ifndef KINEMA_BUILD_DATA_DIR
// Build-system bug: this symbol is supposed to be baked in from
// CMake. Define as empty so the code below simply skips the
// build-tree fallback rather than failing to compile.
#define KINEMA_BUILD_DATA_DIR ""
#endif

namespace kinema::core::mpv_config {

namespace {

constexpr auto kSubPath = "kinema/mpv";

QString resolveFromStandardPaths()
{
    // LocateDirectory searches every path in the app's
    // GenericDataLocation list (incl. $XDG_DATA_HOME and every entry
    // in $XDG_DATA_DIRS) and returns the first match. Empty on miss.
    return QStandardPaths::locate(
        QStandardPaths::GenericDataLocation,
        QString::fromLatin1(kSubPath),
        QStandardPaths::LocateDirectory);
}

QString resolveFromBuildTree()
{
    const QString buildDataDir
        = QString::fromUtf8(KINEMA_BUILD_DATA_DIR);
    if (buildDataDir.isEmpty()) {
        return {};
    }
    const QString candidate = buildDataDir + QLatin1String("/")
        + QString::fromLatin1(kSubPath);
    if (QFileInfo(candidate).isDir()) {
        // Canonicalise so downstream logs read cleanly.
        return QDir(candidate).absolutePath();
    }
    return {};
}

} // namespace

QString dataDir()
{
    const QString fromStd = resolveFromStandardPaths();
    if (!fromStd.isEmpty()) {
        return fromStd;
    }
    return resolveFromBuildTree();
}

QString mpvConfPath()
{
    const QString base = dataDir();
    if (base.isEmpty()) {
        return {};
    }
    const QString p = base + QLatin1String("/mpv.conf");
    return QFileInfo::exists(p) ? p : QString {};
}

QString inputConfPath()
{
    const QString base = dataDir();
    if (base.isEmpty()) {
        return {};
    }
    const QString p = base + QLatin1String("/input.conf");
    return QFileInfo::exists(p) ? p : QString {};
}

} // namespace kinema::core::mpv_config
