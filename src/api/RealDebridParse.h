// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/RealDebrid.h"

#include <QJsonDocument>

namespace kinema::api::realdebrid {

/**
 * Parse a Real-Debrid `/rest/1.0/user` response body.
 *
 * The schema is documented at https://api.real-debrid.com/, §GET /user.
 * Shape:
 *   {
 *     "id": 12345,
 *     "username": "alice",
 *     "email": "a@example.com",
 *     "points": 100,
 *     "locale": "en",
 *     "avatar": "https://…",
 *     "type": "premium",
 *     "premium": 7776000,
 *     "expiration": "2026-11-14T00:00:00.000Z"
 *   }
 *
 * Free accounts omit "expiration" (or send an epoch in the past).
 *
 * Throws HttpError(Kind::Json) if the top-level shape is wrong.
 */
RealDebridUser parseUser(const QJsonDocument& doc);

} // namespace kinema::api::realdebrid
