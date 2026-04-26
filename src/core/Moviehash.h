// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QByteArray>
#include <QString>

namespace kinema::core::moviehash {

/**
 * OpenSubtitles "movie hash" — a 64-bit fingerprint over the first
 * 64 KB and the last 64 KB of a media file. Used by the OpenSubtitles
 * search API as an optional ranking boost: results whose
 * uploader-supplied hash matches come back with
 * `attributes.moviehash_match: true` so the picker can sort them to
 * the top.
 *
 * Algorithm (per the wiki):
 *   1. Sum every 8-byte little-endian word of the first 64 KB.
 *   2. Sum every 8-byte little-endian word of the last 64 KB.
 *   3. Add the file size in bytes.
 *   The 64-bit sum (allowed to wrap) is the hash.
 *
 * `head` and `tail` must each be exactly 65536 bytes. Returns an
 * empty string when the inputs don't satisfy that — callers treat an
 * empty hash as "couldn't compute" and silently drop it.
 */
QString compute(const QByteArray& head,
    const QByteArray& tail, qint64 totalSize);

} // namespace kinema::core::moviehash
