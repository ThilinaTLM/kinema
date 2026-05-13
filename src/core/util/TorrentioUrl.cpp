// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/util/TorrentioUrl.h"

#include <QRegularExpression>
#include <QStringList>

namespace kinema::core::torrentio {

namespace {

bool hostLooksLikeTorrentio(const QString& host)
{
    if (host.isEmpty()) {
        return false;
    }
    // Production host + subdomains.
    if (host == QLatin1String("torrentio.strem.fun")
        || host.endsWith(QLatin1String(".torrentio.strem.fun"))) {
        return true;
    }
    // Self-hosted mirrors typically keep "torrentio" in the host
    // name (e.g. `torrentio.example.com`, `my-torrentio.dev`).
    return host.contains(QLatin1String("torrentio"), Qt::CaseInsensitive);
}

bool isHash40(const QString& s)
{
    static const QRegularExpression re(QStringLiteral("^[0-9a-fA-F]{40}$"));
    return re.match(s).hasMatch();
}

} // namespace

std::optional<ResolveUrlInfo> parseResolveUrl(const QUrl& u)
{
    if (!u.isValid()) {
        return std::nullopt;
    }
    const auto scheme = u.scheme().toLower();
    if (scheme != QLatin1String("http")
        && scheme != QLatin1String("https")) {
        return std::nullopt;
    }
    if (!hostLooksLikeTorrentio(u.host())) {
        return std::nullopt;
    }

    // Use `PrettyDecoded` so percent-encoded filename segments come back
    // human-readable; we still split before any further decoding to keep
    // segment boundaries intact.
    const QString path = u.path(QUrl::FullyDecoded);
    QStringList segments
        = path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    // Expected: resolve, provider, token, hash, file, idx, file
    if (segments.size() != 7) {
        return std::nullopt;
    }
    if (segments[0] != QLatin1String("resolve")) {
        return std::nullopt;
    }
    // segments[1] = provider, segments[2] = token — intentionally ignored.
    const QString& hash = segments[3];
    if (!isHash40(hash)) {
        return std::nullopt;
    }
    bool idxOk = false;
    const int fileIndex = segments[5].toInt(&idxOk);
    if (!idxOk || fileIndex < 0) {
        return std::nullopt;
    }

    ResolveUrlInfo info;
    info.infoHash = hash.toLower();
    info.fileIndex = fileIndex;
    // Prefer the trailing filename segment (Torrentio repeats it; the
    // second copy is what mpv would see as the basename). Fall back to
    // segment[4] if the trailing one is empty for any reason.
    info.fileNameHint
        = segments[6].isEmpty() ? segments[4] : segments[6];
    return info;
}

} // namespace kinema::core::torrentio
