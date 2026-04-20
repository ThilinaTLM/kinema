// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/RealDebridParse.h"

#include "core/HttpError.h"

#include <KLocalizedString>

#include <QJsonObject>
#include <QJsonValue>

namespace kinema::api::realdebrid {

RealDebridUser parseUser(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid /user response was not a JSON object."));
    }
    const auto obj = doc.object();

    RealDebridUser u;
    u.username = obj.value(QStringLiteral("username")).toString();
    u.email = obj.value(QStringLiteral("email")).toString();
    u.type = obj.value(QStringLiteral("type")).toString();

    // "expiration" is ISO-8601. Missing or unparseable → leave as nullopt.
    const auto exp = obj.value(QStringLiteral("expiration")).toString();
    if (!exp.isEmpty()) {
        const auto dt = QDateTime::fromString(exp, Qt::ISODate);
        if (dt.isValid()) {
            u.premiumUntil = dt;
        }
    }
    return u;
}

} // namespace kinema::api::realdebrid
