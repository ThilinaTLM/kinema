// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>
#include <QStringList>

namespace kinema::playback::ports {

/**
 * Stricter port for subtitle integrations. Currently a thin
 * collection of side-effects used by `SubtitleSessionService`:
 *  - update the active set of subtitle paths (for badge state);
 *  - apply a moviehash to enable hash-based search ranking.
 *
 * The OpenSubtitles HTTP client / cache store stay in
 * `controllers::SubtitleController` for now; this port can grow
 * later if those need to move behind a stricter boundary.
 */
class SubtitlePort
{
public:
    virtual ~SubtitlePort() = default;

    virtual void setActiveSubtitlePaths(const QStringList& paths) = 0;
    virtual void setMoviehash(const QString& hex) = 0;
    virtual void clearMoviehash() = 0;
};

} // namespace kinema::playback::ports
