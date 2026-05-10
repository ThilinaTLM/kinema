// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Debrid.h"

namespace kinema::api {

QString providerToString(DebridProvider p)
{
    switch (p) {
    case DebridProvider::RealDebrid:
        return QStringLiteral("realdebrid");
    case DebridProvider::AllDebrid:
        return QStringLiteral("alldebrid");
    case DebridProvider::None:
        break;
    }
    return QStringLiteral("none");
}

DebridProvider providerFromString(const QString& s)
{
    const auto v = s.trimmed().toLower();
    if (v == QLatin1String("realdebrid")) {
        return DebridProvider::RealDebrid;
    }
    if (v == QLatin1String("alldebrid")) {
        return DebridProvider::AllDebrid;
    }
    return DebridProvider::None;
}

} // namespace kinema::api
