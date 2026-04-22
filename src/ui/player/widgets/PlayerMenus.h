// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/MpvTrackList.h"

#include <functional>

class QMenu;

namespace kinema::ui::player::widgets::menus {

/**
 * Populate `menu` with one item per audio track in `tracks`. The
 * currently-selected track is check-marked. Triggers invoke
 * `onSelect(trackId)` with the mpv track id; `-1` represents "none"
 * when such an entry is surfaced (not used for audio today but
 * reserved so the client code can handle both families symmetrically).
 *
 * The function clears `menu` first so callers can rebuild it on every
 * track-list change without leaking old actions.
 */
void populateAudioMenu(QMenu* menu,
    const core::tracks::TrackList& tracks,
    std::function<void(int trackId)> onSelect);

/**
 * Populate `menu` with the subtitle tracks plus a leading "None"
 * entry that disables subtitles (`onSelect(-1)`). Currently-active
 * track (including "None" when no sub is selected) is check-marked.
 */
void populateSubtitleMenu(QMenu* menu,
    const core::tracks::TrackList& tracks,
    std::function<void(int trackId)> onSelect);

/**
 * Fixed ladder of playback speeds: 0.5 / 0.75 / 1.0 / 1.25 / 1.5 / 2.
 * The entry closest to `current` (within 0.01) is check-marked.
 * Triggers call `onSelect(factor)`.
 */
void populateSpeedMenu(QMenu* menu, double current,
    std::function<void(double factor)> onSelect);

} // namespace kinema::ui::player::widgets::menus
