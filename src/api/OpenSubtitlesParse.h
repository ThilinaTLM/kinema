// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Subtitle.h"

#include <QJsonDocument>
#include <QList>
#include <QString>

namespace kinema::api::opensubtitles {

/**
 * Parse a `GET /subtitles` response body into a list of
 * `SubtitleHit`s. Pulls `language`, `release`, `download_count`,
 * `ratings`, `hearing_impaired`, `foreign_parts_only`,
 * `moviehash_match`, `files[0].file_id`, `files[0].file_name` from
 * each `data[i].attributes` block. Throws `std::runtime_error` when
 * the top-level shape isn't an object with a `data` array.
 */
QList<SubtitleHit> parseSearch(const QJsonDocument& doc);

/**
 * Parse a `POST /download` response into a `SubtitleDownloadTicket`.
 * Throws `std::runtime_error` on shape failure. Quota-exhausted (HTTP
 * 406 / `remaining == 0` with `link` empty) is signalled by leaving
 * `link` empty; the controller surfaces a quota error to the user
 * using the `resetAt` field for context.
 */
SubtitleDownloadTicket parseDownload(const QJsonDocument& doc);

/**
 * Parse a `POST /login` response. Returns the bearer token
 * (`token` field). Throws `std::runtime_error` on shape failure.
 */
QString parseLogin(const QJsonDocument& doc);

} // namespace kinema::api::opensubtitles
