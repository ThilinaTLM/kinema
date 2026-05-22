// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

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
Tokens parse(const domain::Stream& s);

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

// --- pack classifier ------------------------------------------------

/// Heuristic classification of "is this stream row a multi-episode
/// torrent?" derived purely from `domain::Stream` fields. The
/// authoritative answer ("does episode N+1 actually exist inside?")
/// can only come from the libtorrent file list once the torrent has
/// metadata; this classifier exists so the stream picker can hint at
/// it *before* the user commits to downloading.
///
/// `kind == MediaKind::Movie` always classifies as `None` — multi-part
/// movie releases are not the same affordance as series packs and
/// must not show a "Season pack" chip in the movie stream picker.
enum class PackKind {
    None,         ///< single-episode (or unknown — be conservative)
    MultiEpisode, ///< pack spans multiple episodes; scope unclear
    Season,       ///< explicit "Season N" / "S0N Complete" claim
    MultiSeason,  ///< "S01-S05" / "Complete Series" claim
};

struct PackHint {
    PackKind kind = PackKind::None;
    /// Free-form claim string captured from the release name,
    /// e.g. "Season 1", "S01 Complete", "S01-S05". Empty when the
    /// only signal was `fileIndex >= 0` with no textual hint.
    /// Trimmed and capped to a tooltip-friendly length.
    QString claim;
};

/// Classify a stream as a pack. Pure, no I/O, safe to call per-row.
/// Movies always return `PackKind::None`.
PackHint classifyPack(const domain::Stream& s, domain::MediaKind kind);

/// Short localized chip label ("Season pack", "Complete series",
/// "Multi-episode"). Empty for `PackKind::None`.
QString packLabel(PackKind k);

/// Stable QML-side token: "none" / "multi" / "season" / "multiseason".
/// Mirrors the `debridProvider` role convention so QML can compare by
/// string instead of leaking Qt enums.
QString packKindToken(PackKind k);

} // namespace kinema::core::stream_tokens
