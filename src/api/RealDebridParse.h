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
 * Free accounts omit "expiration" (or send an epoch in the past).
 *
 * Throws HttpError(Kind::Json) if the top-level shape is wrong.
 */
RealDebridUser parseUser(const QJsonDocument& doc);

/**
 * Parse `GET /torrents/instantAvailability/{hash}` response.
 *
 * Real-Debrid replies with a per-host map keyed by lowercase info-hash:
 *   {
 *     "<hash>": {
 *       "rd": [
 *         { "<fileId>": { "filename": "...", "filesize": 12345 }, ... },
 *         { "<fileId>": ... }
 *       ]
 *     }
 *   }
 *
 * An empty object (or a hash whose `rd` array is empty/missing) means
 * "not cached". Non-rd hosts are ignored — Kinema only consumes RD.
 */
RdInstantAvailability parseInstantAvailability(const QJsonDocument& doc,
    const QString& infoHash);

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
