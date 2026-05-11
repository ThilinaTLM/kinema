// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QByteArray>
#include <QList>
#include <QString>

namespace kinema::core::chapters {

/**
 * One entry from mpv's `chapter-list` property. `time` is the
 * chapter's start time in seconds from the file origin; `title` is
 * whatever mpv extracted from the demuxer metadata (often empty).
 */
struct Chapter {
    double time = 0.0;
    QString title;
};

using ChapterList = QList<Chapter>;

/**
 * Parse the JSON representation of `chapter-list`. Accepts both the
 * string form (`mpv_get_property_string`) and the serialised node
 * form — any well-formed JSON array of `{ "time": <sec>, "title":
 * <string> }` entries works. Fatal parse errors yield an empty list
 * (callers keep a chapter-less seek bar); malformed per-entry values
 * are silently dropped.
 *
 * Chapters are **not sorted** — mpv guarantees file-order already and
 * chapters are almost always monotonically increasing. Callers that
 * need a guaranteed sort should run `std::sort` on the result.
 */
ChapterList parseChapterList(const QByteArray& json);

/// Extract just the chapter start times (seconds), preserving input
/// order. Convenience for the seek bar which only needs the ticks.
QList<double> chapterTimes(const ChapterList& chapters);

} // namespace kinema::core::chapters
