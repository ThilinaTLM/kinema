// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QByteArray>
#include <QList>
#include <QString>

#include <optional>

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

/**
 * Best-effort "where do the end credits start?" inference from a
 * file's chapter metadata. Used by `controllers::HistoryController`
 * to auto-mark a movie / episode watched when the user stops past
 * the credits boundary, even if they're below the percentage
 * threshold.
 *
 * Rules (first match wins):
 *   1. **Titled credit chapter** — any chapter whose title matches
 *      `\b(end\s*credits?|credits|ending|end\s*titles?|end\s*card)\b`
 *      (case-insensitive). Returns its `time`.
 *   2. **Untitled last-chapter heuristic** — when the *last* chapter
 *      in the list starts within `[0.80 * duration, 0.97 * duration]`
 *      it is most likely the credits cut. Returns its `time`.
 *   3. Otherwise `std::nullopt`.
 *
 * `duration` must be in seconds. A zero or negative duration
 * disables the untitled heuristic but still honours rule (1).
 */
std::optional<double> findCreditsStart(const ChapterList& chapters,
    double duration);

} // namespace kinema::core::chapters
