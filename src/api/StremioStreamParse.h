// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QJsonDocument>

namespace kinema::api::stremio {

/**
 * Parse a Stremio addon `/stream/{kind}/{id}.json` response body.
 *
 * This is the shared parser used by every `Indexer` that speaks the
 * Stremio addon protocol — currently `TorrentioIndexer` and
 * `MediaFusionIndexer`. Both services emit the same JSON shape and the
 * same emoji-prefixed metadata convention in `title`:
 *
 *     "The Matrix 1999 1080p BluRay x264-NOGRP\n👤 1500 💾 2.1 GB ⚙️ ThePirateBay"
 *
 * Pulls seeders / size / provider out of the free-form multi-line
 * `title` field using tolerant regexes — any field that doesn't match
 * is simply left unset. Rows with neither an `infoHash` nor a
 * `directUrl` are dropped (not actionable downstream).
 *
 * Throws `core::HttpError(Kind::Json)` if the top level isn't a JSON
 * object. A missing or non-array `streams` field returns an empty list.
 */
QList<Stream> parseStreams(const QJsonDocument& doc);

} // namespace kinema::api::stremio
