// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QJsonDocument>

namespace kinema::api::torrentio {

/**
 * Parse a Torrentio `/stream/{kind}/{id}.json` response body.
 *
 * Pulls seeders/size/provider out of the free-form multi-line `title` field
 * using tolerant regexes — any field that doesn't match is simply left
 * unset.
 *
 * Throws HttpError(Kind::Json) if the top level isn't a JSON object.
 */
QList<Stream> parseStreams(const QJsonDocument& doc);

} // namespace kinema::api::torrentio
