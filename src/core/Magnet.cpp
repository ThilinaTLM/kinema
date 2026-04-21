// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Magnet.h"

#include <QUrl>

namespace kinema::core::magnet {

QStringList defaultTrackers()
{
    QStringList out;
    out.reserve(static_cast<int>(kDefaultTrackers.size()));
    for (const char* t : kDefaultTrackers) {
        out.append(QString::fromLatin1(t));
    }
    return out;
}

QString build(QStringView infoHash, QStringView displayName, const QStringList& trackers)
{
    QString out;
    out.reserve(256);
    out += QStringLiteral("magnet:?xt=urn:btih:");
    out += infoHash.toString();

    if (!displayName.isEmpty()) {
        out += QStringLiteral("&dn=");
        out += QString::fromUtf8(
            QUrl::toPercentEncoding(displayName.toString(), QByteArray {}, QByteArray { " " }));
    }

    for (const QString& tr : trackers) {
        out += QStringLiteral("&tr=");
        out += QString::fromUtf8(QUrl::toPercentEncoding(tr));
    }

    return out;
}

} // namespace kinema::core::magnet
