// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/MediaFusionParse.h"

#include <QUrl>

namespace kinema::api::mediafusion {

namespace {

QString stripTrailingSlashes(QString s)
{
    while (s.endsWith(QLatin1Char('/'))) {
        s.chop(1);
    }
    return s;
}

} // namespace

ManifestUrlParts parseManifestUrl(const QString& urlStr)
{
    const QUrl url(urlStr.trimmed());
    if (!url.isValid() || url.host().isEmpty() || url.scheme().isEmpty()) {
        return {};
    }

    // Strip query / fragment — the manifest URL we accept doesn't use them.
    QString path = url.path();
    if (!path.endsWith(QLatin1String("/manifest.json"))) {
        return {};
    }
    // Drop "/manifest.json" suffix.
    path.chop(QStringLiteral("/manifest.json").size());

    // What's left is the optional encrypted-config segment. Leading
    // slash is OK; we tolerate but don't require it.
    QString token = path;
    while (token.startsWith(QLatin1Char('/'))) {
        token.remove(0, 1);
    }
    // Reject anything with more than one segment — the manifest URL
    // shape is exactly `<host>[/<token>]/manifest.json`.
    if (token.contains(QLatin1Char('/'))) {
        return {};
    }

    ManifestUrlParts out;
    out.valid = true;
    out.baseUrl = url.scheme() + QStringLiteral("://") + url.host();
    if (url.port() != -1) {
        out.baseUrl += QLatin1Char(':') + QString::number(url.port());
    }
    out.encryptedConfig = token;
    return out;
}

QString buildStreamUrl(const ManifestUrlParts& parts,
    const QString& kindStr,
    const QString& streamId)
{
    if (!parts.valid) {
        return {};
    }
    QString out = stripTrailingSlashes(parts.baseUrl);
    if (!parts.encryptedConfig.isEmpty()) {
        out += QLatin1Char('/') + parts.encryptedConfig;
    }
    out += QStringLiteral("/stream/") + kindStr + QLatin1Char('/') + streamId
        + QStringLiteral(".json");
    return out;
}

} // namespace kinema::api::mediafusion
