// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/RealDebrid.h"

#include <QJsonDocument>

namespace kinema::api::realdebrid {
using namespace kinema::domain;

/**
 * Parse a Real-Debrid `/rest/1.0/user` response body.
 *
 * The schema is documented at https://api.real-debrid.com/, §GET /user.
 * Free accounts omit "expiration" (or send an epoch in the past).
 *
 * Throws HttpError(Kind::Json) if the top-level shape is wrong.
 */
RealDebridUser parseUser(const QJsonDocument& doc);

/**
 * Parse `POST /torrents/addMagnet` response: { id, uri }.
 */
RdAddMagnetResult parseAddMagnet(const QJsonDocument& doc);

/**
 * Parse `GET /torrents/info/{id}` response.
 */
RdTorrentInfo parseTorrentInfo(const QJsonDocument& doc);

/**
 * Parse `POST /unrestrict/link` response.
 */
RdUnrestrictedLink parseUnrestrictedLink(const QJsonDocument& doc);

} // namespace kinema::api::realdebrid
