// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QByteArray>
#include <QString>

namespace kinema::core::media_chips {

/// Inputs for the media-info chip row rendered above the title
/// strip. Fields left at their zero/empty default are skipped when
/// building the chip list, so callers can pass whatever subset
/// libmpv has reported so far without branching.
struct ChipInputs {
    int     videoHeight   = 0;   ///< from mpv `height`
    QString videoCodec;          ///< from mpv `video-codec`
    QString audioCodec;          ///< from mpv `audio-codec`
    int     audioChannels = 0;   ///< from `audio-params/channel-count`
    QString hdrPrimaries;        ///< from `video-params/primaries`
    QString hdrGamma;            ///< from `video-params/gamma`
};

/// Build a compact JSON array (e.g. `["4K","HDR10","H.265",
/// "EAC3 5.1"]`) suitable for the `set-media-chips` IPC command.
///
/// Rules (all documented in the unit test):
///   - Resolution:  `"4K"` when height >= 2000,
///                  `"<n>p"` for standard tiers (1440, 1080, 720,
///                  480, 360), otherwise `"<n>p"` with the raw
///                  height. Omitted when height <= 0.
///   - HDR:         `"HDR10"` for bt.2020 + pq,
///                  `"HLG"`   for bt.2020 + hlg,
///                  `"DV"`    for gamma/primaries carrying a
///                            `dolbyvision` marker (mpv exposes
///                            this via `video-params/primaries =
///                            dci-p3` + layer side data; we match
///                            on the string name when present).
///                  SDR is intentionally not tagged.
///   - Video codec: friendly mapping (H.265, H.264, AV1, VP9) with
///                  uppercase fallback. Omitted when empty.
///   - Audio:       `"<CODEC> <n>.<sub>"` using standard stereo
///                  (2.0) / 5.1 / 7.1 / mono (1.0) conventions.
///                  When channel count is 0 or unknown, the codec
///                  string alone is emitted (if non-empty).
QByteArray toIpcJson(const ChipInputs& in);

} // namespace kinema::core::media_chips
