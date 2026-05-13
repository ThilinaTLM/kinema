// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/DebridCredentials.h"

namespace kinema::domain {

QString providerToUrlToken(DebridProvider p)
{
    switch (p) {
    case DebridProvider::RealDebrid:
        return QStringLiteral("realdebrid");
    case DebridProvider::AllDebrid:
        return QStringLiteral("alldebrid");
    case DebridProvider::None:
        break;
    }
    return {};
}

} // namespace kinema::domain
