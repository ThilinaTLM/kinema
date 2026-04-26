// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/UrlRedactor.h"

#include <QRegularExpression>

namespace kinema::core {

QString redactUrlForLog(const QUrl& url)
{
    if (!url.isValid()) {
        return QStringLiteral("<invalid-url>");
    }

    // Keep the same useful context we logged before (scheme/host/path), but
    // never include query strings, fragments, or URL userinfo. Query strings
    // are removed entirely because many APIs use inconsistent names for
    // credentials; preserving them is not worth the leak risk.
    QString text = url.toString(QUrl::RemoveQuery
        | QUrl::RemoveFragment
        | QUrl::RemoveUserInfo);

    // Torrentio direct resolve URLs include the Real-Debrid token as a path
    // segment: /resolve/realdebrid/<token>/...
    static const QRegularExpression rdResolve(
        QStringLiteral("(/resolve/realdebrid/)[^/?#]+"),
        QRegularExpression::CaseInsensitiveOption);
    text.replace(rdResolve, QStringLiteral("\\1<redacted>"));

    // Torrentio addon config paths can contain realdebrid=<token> inside the
    // first path segment, either decoded or percent-encoded depending on how
    // the QUrl was constructed.
    static const QRegularExpression rdConfig(
        QStringLiteral("(realdebrid=)[^|/?#]+"),
        QRegularExpression::CaseInsensitiveOption);
    text.replace(rdConfig, QStringLiteral("\\1<redacted>"));

    static const QRegularExpression rdConfigEncoded(
        QStringLiteral("(realdebrid%3D)[^%/?#]+"),
        QRegularExpression::CaseInsensitiveOption);
    text.replace(rdConfigEncoded, QStringLiteral("\\1<redacted>"));

    return text;
}

} // namespace kinema::core
