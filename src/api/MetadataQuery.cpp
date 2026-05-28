// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/MetadataQuery.h"

#include <QRegularExpression>
#include <QUrl>

namespace kinema::api::metadata_query {

std::optional<QString> extractImdbTitleId(QStringView input)
{
    const auto text = input.toString().trimmed();
    if (text.isEmpty()) {
        return std::nullopt;
    }

    static const QRegularExpression bareId(
        QStringLiteral("^tt\\d{5,}$"),
        QRegularExpression::CaseInsensitiveOption);
    const auto bareMatch = bareId.match(text);
    if (bareMatch.hasMatch()) {
        return bareMatch.captured(0).toLower();
    }

    const QUrl url(text);
    if (!url.isValid() || url.host().isEmpty()) {
        return std::nullopt;
    }

    const auto host = url.host().toLower();
    if (host != QStringLiteral("imdb.com")
        && !host.endsWith(QStringLiteral(".imdb.com"))) {
        return std::nullopt;
    }

    static const QRegularExpression titlePath(
        QStringLiteral("(?:^|/)title/(tt\\d{5,})(?:/|$)"),
        QRegularExpression::CaseInsensitiveOption);
    const auto pathMatch = titlePath.match(url.path());
    if (!pathMatch.hasMatch()) {
        return std::nullopt;
    }
    return pathMatch.captured(1).toLower();
}

bool isImdbTitleId(QStringView input)
{
    return extractImdbTitleId(input).has_value();
}

} // namespace kinema::api::metadata_query
