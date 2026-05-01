// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/AppIconResolver.h"

#include <QDir>
#include <QFile>
#include <QStandardPaths>
#include <QUrl>

namespace kinema::ui::qml {

namespace {

QString sourcePathFor(const QString& iconId)
{
    return QStringLiteral(
               ":/qt/qml/dev/tlmtech/kinema/app/ui/qml/icons/lucide/")
        + iconId + QStringLiteral(".svg");
}

QString cacheDirPath()
{
    const QString base = QStandardPaths::writableLocation(
        QStandardPaths::CacheLocation);
    return QDir(base).filePath(QStringLiteral("app-icons"));
}

QString sanitizeColor(const QString& hexColor)
{
    QString out = hexColor.trimmed().toLower();
    if (out.startsWith(QLatin1Char('#'))) {
        out.remove(0, 1);
    }
    return out;
}

} // namespace

AppIconResolver::AppIconResolver(QObject* parent)
    : QObject(parent)
{
}

QString AppIconResolver::url(const QString& iconId,
    const QString& hexColor) const
{
    const QString path = ensureIconFile(iconId, hexColor);
    return path.isEmpty() ? QString {} : QUrl::fromLocalFile(path).toString();
}

QString AppIconResolver::ensureIconFile(const QString& iconId,
    const QString& hexColor) const
{
    if (iconId.isEmpty()) {
        return {};
    }

    QFile sourceFile(sourcePathFor(iconId));
    if (!sourceFile.open(QIODevice::ReadOnly)) {
        return {};
    }

    const QString color = sanitizeColor(hexColor);
    if (color.size() != 6) {
        return {};
    }

    QByteArray svg = sourceFile.readAll();
    const QByteArray replacement = QByteArray("#") + color.toUtf8();
    svg.replace(QByteArrayView("currentColor"), QByteArrayView(replacement));

    QDir dir(cacheDirPath());
    if (!dir.mkpath(QStringLiteral("."))) {
        return {};
    }

    const QString fileName = iconId + QStringLiteral("-")
        + color + QStringLiteral(".svg");
    const QString outPath = dir.filePath(fileName);

    QFile outFile(outPath);
    if (outFile.exists()) {
        if (outFile.open(QIODevice::ReadOnly)) {
            if (outFile.readAll() == svg) {
                return outPath;
            }
            outFile.close();
        }
    }

    if (!outFile.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        return {};
    }
    if (outFile.write(svg) != svg.size()) {
        return {};
    }

    return outPath;
}

} // namespace kinema::ui::qml
