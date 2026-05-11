// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/DebridFilePicker.h"

#include <QFileInfo>
#include <QStringList>

namespace kinema::download::picker {

int score(const QString& path,
    qint64 sizeBytes,
    const domain::AssetRef& ref)
{
    int s = 0;

    if (sizeBytes >= 50LL * 1024LL * 1024LL) {
        s += 2;
    }

    const auto suffix = QFileInfo(path).suffix().toLower();
    static const QStringList kVideo {
        QStringLiteral("mkv"), QStringLiteral("mp4"),
        QStringLiteral("m4v"), QStringLiteral("avi"),
        QStringLiteral("mov"), QStringLiteral("ts"),
    };
    if (kVideo.contains(suffix)) {
        s += 4;
    }

    if (!ref.fileNameHint.isEmpty()) {
        const auto base = QFileInfo(path).fileName();
        if (base.compare(ref.fileNameHint, Qt::CaseInsensitive) == 0) {
            s += 16;
        } else if (base.contains(ref.fileNameHint, Qt::CaseInsensitive)) {
            s += 8;
        }
    }

    if (ref.key.season.has_value() && ref.key.episode.has_value()) {
        const auto s2 = QStringLiteral("S%1").arg(*ref.key.season,
            2, 10, QLatin1Char('0'));
        const auto s2u = QStringLiteral("%1x").arg(*ref.key.season,
            2, 10, QLatin1Char('0'));
        const auto e2 = QStringLiteral("E%1").arg(*ref.key.episode,
            2, 10, QLatin1Char('0'));
        if (path.contains(s2 + e2, Qt::CaseInsensitive)) {
            s += 8;
        } else if (path.contains(s2u, Qt::CaseInsensitive)
            && path.contains(QString::number(*ref.key.episode))) {
            s += 4;
        }
    }
    return s;
}

int chooseIndex(const QList<Candidate>& candidates,
    const domain::AssetRef& ref)
{
    int bestScore = -1;
    int bestIdx = -1;
    qint64 bestSize = -1;

    for (int i = 0; i < candidates.size(); ++i) {
        const auto& c = candidates[i];
        const int sc = score(c.path, c.bytes, ref);
        if (sc > bestScore || (sc == bestScore && c.bytes > bestSize)) {
            bestScore = sc;
            bestSize = c.bytes;
            bestIdx = i;
        }
    }
    return bestIdx;
}

} // namespace kinema::download::picker
