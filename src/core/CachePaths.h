// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDir>

namespace kinema::core::cache {

/**
 * Cross-platform anchor for everything we cache on disk. Built on top
 * of `QStandardPaths::CacheLocation` (`~/.cache/kinema/` on XDG
 * desktops). The directory is created lazily on first call.
 *
 * Sub-modules — subtitles today, video / posters tomorrow — sit
 * underneath. No state, no QObject; cheap to call repeatedly.
 */
QDir root();

/// `<root>/subtitles`. Sharded one level deeper by IMDb id at
/// individual file path build time (see SubtitleCacheStore).
QDir subtitlesDir();

/// Recursive byte size of `dir` (files only). Returns 0 when the
/// directory does not exist.
qint64 dirSizeBytes(const QDir& dir);

} // namespace kinema::core::cache
