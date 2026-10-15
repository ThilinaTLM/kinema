// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QMetaType>
#include <QString>

#include <optional>

namespace kinema::domain {

/**
 * Generic, backend-agnostic description of one file that lives
 * inside a media-source session (torrent, debrid pack, …).
 *
 * `MediaFileSelectionPolicy` and the series adjacency code operate
 * over `MediaFileEntry` so they no longer care whether the entries
 * came from libtorrent or from a debrid resolver.
 */
struct MediaFileEntry {
    int index = -1; ///< 0-based file index inside the parent session
    QString path; ///< full relative path including subdirectories
    qint64 size = 0; ///< file size in bytes (0 when unknown)
    bool playable = true; ///< pre-filtered playable hint (defaults true)
};

/// One concrete episode file inside a season pack.
struct EpisodeFileTarget {
    int season = 0;
    int episode = 0;
    MediaFileEntry file;
};

/// Previous / current / next adjacency for the episode currently
/// playing inside a season-pack session. Either side may be empty
/// when the pack does not carry that episode, or the matching is
/// ambiguous.
struct EpisodeAdjacency {
    std::optional<EpisodeFileTarget> previous;
    std::optional<EpisodeFileTarget> current;
    std::optional<EpisodeFileTarget> next;

    bool navigationAvailable() const noexcept
    {
        return current.has_value()
            && (previous.has_value() || next.has_value());
    }
};

} // namespace kinema::domain

Q_DECLARE_METATYPE(kinema::domain::MediaFileEntry)
Q_DECLARE_METATYPE(kinema::domain::EpisodeFileTarget)
Q_DECLARE_METATYPE(kinema::domain::EpisodeAdjacency)
