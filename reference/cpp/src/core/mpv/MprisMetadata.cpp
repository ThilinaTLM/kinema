// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/mpv/MprisMetadata.h"

#include <QCryptographicHash>
#include <QDBusObjectPath>

namespace kinema::core::mpris {

QString trackObjectPath(const domain::PlaybackKey& key)
{
    const QByteArray stableKey = key.storageKey().toUtf8();
    const QByteArray hash = QCryptographicHash::hash(
        stableKey, QCryptographicHash::Sha1).toHex();
    return QStringLiteral("/dev/tlmtech/kinema/track/")
        + QString::fromLatin1(hash);
}

QVariantMap metadata(const domain::PlaybackContext& ctx,
    double durationSeconds)
{
    QVariantMap out;
    out.insert(QStringLiteral("mpris:trackid"),
        QVariant::fromValue(QDBusObjectPath(trackObjectPath(ctx.key))));
    if (durationSeconds > 0.0) {
        out.insert(QStringLiteral("mpris:length"),
            static_cast<qlonglong>(durationSeconds * 1000000.0));
    }
    if (!ctx.poster.isEmpty()) {
        out.insert(QStringLiteral("mpris:artUrl"),
            ctx.poster.toString());
    }
    if (!ctx.title.isEmpty()) {
        out.insert(QStringLiteral("xesam:title"), ctx.title);
    }
    if (!ctx.seriesTitle.isEmpty()) {
        out.insert(QStringLiteral("xesam:album"), ctx.seriesTitle);
    }
    return out;
}

} // namespace kinema::core::mpris
