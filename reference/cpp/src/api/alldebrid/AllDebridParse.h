// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/AllDebrid.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QList>

namespace kinema::api::alldebrid {
using namespace kinema::domain;

/**
 * Validate the AllDebrid response envelope and return the inner
 * `data` object.
 *
 *   { "status": "success", "data": { ... } }
 *   { "status": "error",   "error": { "code": "...", "message": "..." } }
 *
 * On `status:"error"` this throws `core::HttpError(Kind::HttpStatus)`
 * with a status of 400 (or 401 for auth codes) and a localised
 * message that includes the upstream code, so the UI surfaces a
 * recognisable failure ("AllDebrid: AUTH_BAD_APIKEY \u2014 The auth
 * apikey is invalid").
 *
 * On a malformed envelope (no `status` key, etc.) throws
 * `Kind::Json`.
 *
 * The `endpointLabel` argument prefixes the thrown message so the
 * log line names the failing endpoint without mentioning the URL.
 */
QJsonObject unwrapEnvelope(const QJsonDocument& doc,
    const char* endpointLabel);

AllDebridUser parseUser(const QJsonDocument& doc);

/// Parses the `magnets[]` array. The array always has one entry
/// because the resolver only uploads one magnet at a time. Per-row
/// `error` objects throw `core::HttpError`.
AdAddMagnetResult parseAddMagnet(const QJsonDocument& doc);

/// Parses the first entry of the `magnets[]` array from
/// `/v4.1/magnet/status`. Per-row errors throw.
AdMagnetStatus parseMagnetStatus(const QJsonDocument& doc);

/// Walks the recursive `files` tree and returns every leaf file
/// keyed by full path (folders joined with `/`). Skips entries
/// whose hoster link is empty (folders / non-downloadable nodes).
QList<AdMagnetFile> parseMagnetFiles(const QJsonDocument& doc);

/// Parses POST /v4/link/unlock. When the response carries a
/// `delayed` id and no `link`, the returned struct has
/// `delayedId != 0` and an empty `download`; the client then polls
/// `/v4/link/delayed`.
AdUnlockedLink parseUnlock(const QJsonDocument& doc);

/// Parses POST /v4/link/delayed. Returns the final URL when
/// `status == 2`, throws on `status == 3` (terminal failure), and
/// returns an `AdUnlockedLink` with an empty `download` when
/// `status == 1` (still processing) so callers can keep polling.
AdUnlockedLink parseDelayed(const QJsonDocument& doc);

} // namespace kinema::api::alldebrid
