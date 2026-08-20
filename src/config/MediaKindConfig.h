// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QString>

namespace kinema::config {

/// Serialise a `MediaKind` to the string stored in KConfig
/// (`"Movie"` / `"Series"`).
inline QString mediaKindToConfigString(domain::MediaKind kind)
{
    return kind == domain::MediaKind::Series
        ? QStringLiteral("Series")
        : QStringLiteral("Movie");
}

/// Parse a KConfig `MediaKind` string, defaulting to `Movie`.
inline domain::MediaKind mediaKindFromConfigString(const QString& s)
{
    return s == QLatin1String("Series")
        ? domain::MediaKind::Series
        : domain::MediaKind::Movie;
}

} // namespace kinema::config
