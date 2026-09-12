// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>
#include <QStringList>

namespace kinema::playback::ports {

/** Playback-facing subtitle state commands, independent of presentation. */
class SubtitleSessionPort
{
public:
    virtual ~SubtitleSessionPort() = default;

    virtual void setActiveSubtitlePaths(const QStringList& paths) = 0;
    virtual void setMoviehash(QString hex) = 0;
    virtual void clearMoviehash() = 0;
};

} // namespace kinema::playback::ports
