// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Download.h"

#include <QCryptographicHash>

namespace kinema::domain {

QString assetIdFor(const AssetRef& ref)
{
    // Prefer infoHash + fileIndex \u2014 stable, short, recognisable on disk.
    if (!ref.infoHash.isEmpty()) {
        const auto base = ref.infoHash.toLower();
        if (ref.fileIndex >= 0) {
            return base + QStringLiteral("-f") + QString::number(ref.fileIndex);
        }
        return base;
    }
    // No infoHash \u2014 fall back to a SHA-1 over the discriminating fields.
    QCryptographicHash h(QCryptographicHash::Sha1);
    h.addData(ref.key.storageKey().toUtf8());
    h.addData(QByteArrayLiteral("\x1f"));
    h.addData(ref.releaseName.toUtf8());
    h.addData(QByteArrayLiteral("\x1f"));
    h.addData(QByteArray::number(ref.fileIndex));
    h.addData(QByteArrayLiteral("\x1f"));
    h.addData(ref.fileNameHint.toUtf8());
    return QStringLiteral("h-") + QString::fromLatin1(h.result().toHex());
}

AssetRef assetRefFor(const Stream& s, const PlaybackContext& ctx)
{
    AssetRef r;
    r.key = ctx.key;
    r.infoHash = s.infoHash.trimmed().toLower();
    r.releaseName = s.releaseName;
    r.fileIndex = s.fileIndex;
    r.fileNameHint = s.fileNameHint;
    r.qualityLabel = s.qualityLabel;
    r.resolution = s.resolution;
    r.provider = s.provider;
    r.sizeBytes = s.sizeBytes;
    return r;
}

} // namespace kinema::domain
