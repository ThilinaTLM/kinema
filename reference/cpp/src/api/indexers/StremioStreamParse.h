// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QJsonDocument>

namespace kinema::api::stremio {
using namespace kinema::domain;

/**
 * Parse a Stremio addon `/stream/{kind}/{id}.json` response body.
 *
 * This is the shared parser used by every `Indexer` that speaks the
 * Stremio addon protocol — currently `TorrentioIndexer` and
 * `PeerflixIndexer`. Both services emit the same overall JSON shape;
 * Torrentio embeds emoji-prefixed metadata in the deprecated `title`
 * field, Peerflix uses the current `description` field plus structured
 * numeric fields (`seed`, `sizebytes`, `quality`, `language`):
 *
 *     "The Matrix 1999 1080p BluRay x264-NOGRP\n👤 1500 💾 2.1 GB ⚙️ ThePirateBay"
 *
 * Pulls seeders / size / provider out of the free-form multi-line
 * description using tolerant regexes when no structured fields are
 * present — any field that doesn't match is simply left unset. Rows
 * with neither an `infoHash` nor a `directUrl` are dropped (not
 * actionable downstream).
 *
 * Throws `core::HttpError(Kind::Json)` if the top level isn't a JSON
 * object. A missing or non-array `streams` field returns an empty list.
 */
QList<Stream> parseStreams(const QJsonDocument& doc);

} // namespace kinema::api::stremio
