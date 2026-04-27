// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QString>
#include <QStringList>

namespace kinema::core::stream_tokens {

/**
 * Best-effort categorisation of the noisy strings Torrentio returns.
 *
 * Torrentio packs a lot of decision-driving information (source,
 * codec, HDR profile, audio codec, language hints) into the
 * `qualityLabel` / `releaseName` / `detailsText` fields as
 * unstructured text. The streams page wants those signals as chips
 * and as a compact human summary line, so we parse them once here
 * with regexes that tolerate the variation between providers.
 *
 * Pure and Qt-GUI-free; safe to call from view-models and tests.
 *
 * The parser is deliberately conservative — when a token can't be
 * identified the field stays at its `Unknown` / empty default and
 * the UI degrades gracefully (e.g. omits that piece from the
 * summary). It does not guess.
 */

/// Distribution source.
enum class Source {
    Unknown,
    WebDl,        ///< WEB-DL
    WebRip,       ///< WEBRip / WEB
    BluRayRemux,  ///< BluRay Remux
    BluRay,       ///< BluRay rip
    Hdtv,         ///< HDTV / PDTV
    Dvd,          ///< DVDRip / DVD
    Cam,          ///< CAM / TS / TC / SCR
};

/// Video codec.
enum class Codec {
    Unknown,
    H264,         ///< x264 / AVC / H.264
    H265,         ///< x265 / HEVC / H.265
    AV1,
    VP9,
    Xvid,         ///< XviD / DivX
};

/// HDR profile (mutually exclusive in our taxonomy — DV beats HDR10+ beats HDR10).
enum class Hdr {
    Sdr,
    Hdr10,
    Hdr10Plus,
    DolbyVision,
};

struct Tokens {
    Source source = Source::Unknown;
    Codec codec = Codec::Unknown;
    Hdr hdr = Hdr::Sdr;
    bool tenBit = false;

    /// Audio codec/format hints, in the order they were detected.
    /// Examples: "DDP 5.1", "Atmos", "DTS-HD MA", "TrueHD".
    QStringList audio;

    /// ISO 639-1 codes deduced from flag emoji and textual hints,
    /// deduped, preserving detection order.
    QStringList languages;

    /// True when the row advertises Dual / Multi audio.
    bool multiAudio = false;

    /// Trailing release-group token (e.g. "QxR", "TEPES", "RARBG"),
    /// empty when not parseable.
    QString releaseGroup;
};

/// Parse one stream into structured tokens. Reads `qualityLabel`,
/// `releaseName`, and `detailsText`.
Tokens parse(const api::Stream& s);

/// Localised human label for a `Source` value. Returns an empty
/// string for `Unknown` so callers can omit it from joins.
QString sourceLabel(Source s);

/// Localised human label for a `Codec` value, including the 10-bit
/// suffix when `tenBit` is true (e.g. "x265 10-bit"). Returns an
/// empty string for `Unknown`.
QString codecLabel(Codec c, bool tenBit);

/// Localised human label for an `Hdr` value. Returns an empty
/// string for `Sdr`.
QString hdrLabel(Hdr h);

} // namespace kinema::core::stream_tokens
