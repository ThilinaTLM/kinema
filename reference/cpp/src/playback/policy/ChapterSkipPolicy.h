// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/mpv/MpvChapterList.h"

#include <QString>

#include <optional>

namespace kinema::playback::policy {

enum class SkipKind {
    Intro,
    Outro,
    Credits,
};

struct SkipChapter {
    SkipKind kind = SkipKind::Intro;
    double startSec = 0.0;
    double endSec = 0.0; ///< exclusive
    QString chapterTitle;
};

/// Localised, user-visible button label for a skip prompt.
QString skipButtonLabel(SkipKind kind);

/// Stable diagnostic kind string ("intro" / "outro" / "credits").
QString skipChapterKind(SkipKind kind);

/// Classify a chapter title as intro / outro / credits.
SkipKind classifyChapterTitle(const QString& title);

/// Return the skip chapter (if any) that contains `seconds`,
/// using the provided `chapters` list and stream `duration`.
/// `duration <= 0` is treated as "unknown duration"; the last
/// matching chapter then has no end and is ignored.
std::optional<SkipChapter> activeSkipChapter(
    const core::chapters::ChapterList& chapters,
    double seconds,
    double duration);

} // namespace kinema::playback::policy
