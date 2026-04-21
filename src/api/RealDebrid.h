// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDateTime>
#include <QMetaType>
#include <QString>

#include <optional>

namespace kinema::api {

/// User info returned by the Real-Debrid /user endpoint.
struct RealDebridUser {
    QString username;
    QString email;
    QString type; ///< "premium" or "free"
    std::optional<QDateTime> premiumUntil;
};

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::RealDebridUser)
