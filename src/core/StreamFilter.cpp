// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/StreamFilter.h"

#include "core/StreamTokens.h"

namespace kinema::core::stream_filter {

namespace {

/// Return true when the stream's resolution falls into `token`'s
/// bucket per the user-facing taxonomy in `ClientFilters`.
bool resolutionMatchesToken(const QString& res, const QString& token)
{
    if (token == QLatin1String("4k")) {
        return res == QLatin1String("2160p") || res == QLatin1String("1440p");
    }
    if (token == QLatin1String("1080p")) {
        return res == QLatin1String("1080p");
    }
    if (token == QLatin1String("720p")) {
        return res == QLatin1String("720p");
    }
    if (token == QLatin1String("480p")) {
        return res == QLatin1String("480p");
    }
    if (token == QLatin1String("other")) {
        // Anything not in the named buckets. Covers 360p, "SD",
        // unknown, the "—" placeholder, etc.
        return res != QLatin1String("2160p") && res != QLatin1String("1440p")
            && res != QLatin1String("1080p") && res != QLatin1String("720p")
            && res != QLatin1String("480p");
    }
    return false;
}

/// Map a `ClientFilters::excludedCategories` token to a predicate
/// over the parsed `stream_tokens::Tokens`. Tokens that the parser
/// can't infer from the release name alone (`threed`, `unknown`)
/// stay accepted-but-no-op; `nonen` is honoured for any indexer
/// that populates `Stream::language` (currently Peerflix).
bool categoryMatchesToken(const api::Stream& s, const QString& token)
{
    const auto t = stream_tokens::parse(s);
    if (token == QLatin1String("cam") || token == QLatin1String("scr")) {
        // The parser's `Source::Cam` bucket already covers CAM / TS
        // / TC / SCR releases per the StreamTokens.h taxonomy.
        return t.source == stream_tokens::Source::Cam;
    }
    if (token == QLatin1String("hdr")) {
        return t.hdr != stream_tokens::Hdr::Sdr;
    }
    if (token == QLatin1String("hdr10plus")) {
        return t.hdr == stream_tokens::Hdr::Hdr10Plus;
    }
    if (token == QLatin1String("dolbyvision")) {
        return t.hdr == stream_tokens::Hdr::DolbyVision;
    }
    if (token == QLatin1String("brremux")) {
        return t.source == stream_tokens::Source::BluRayRemux;
    }
    if (token == QLatin1String("nonen")) {
        // Indexers that surface a language code (currently Peerflix)
        // let us drop non-English releases precisely. Indexers that
        // don't (currently Torrentio) leave `Stream::language` empty,
        // in which case we keep the row — Torrentio results pass the
        // URL-level `qualityfilter=nonen` check server-side anyway.
        return !s.language.isEmpty()
            && s.language.compare(QLatin1String("en"), Qt::CaseInsensitive)
                != 0;
    }
    // `threed` and `unknown` are not currently inferrable client-side
    // from the parsed Stream alone; users still get them filtered
    // server-side via Torrentio's URL, and via the keyword blocklist
    // as a fallback.
    return false;
}

} // namespace

bool matchesBlocklist(const api::Stream& s, const QStringList& blocklist)
{
    for (const auto& keyword : blocklist) {
        if (keyword.isEmpty()) {
            continue;
        }
        if (s.releaseName.contains(keyword, Qt::CaseInsensitive)
            || s.detailsText.contains(keyword, Qt::CaseInsensitive)) {
            return true;
        }
    }
    return false;
}

QList<api::Stream> apply(const QList<api::Stream>& in,
    const ClientFilters& filters)
{
    QList<api::Stream> out;
    out.reserve(in.size());
    for (const auto& s : in) {
        if (!filters.keywordBlocklist.isEmpty()
            && matchesBlocklist(s, filters.keywordBlocklist)) {
            continue;
        }

        bool dropped = false;
        for (const auto& tok : filters.excludedResolutions) {
            if (resolutionMatchesToken(s.resolution, tok)) {
                dropped = true;
                break;
            }
        }
        if (dropped) {
            continue;
        }
        for (const auto& tok : filters.excludedCategories) {
            if (categoryMatchesToken(s, tok)) {
                dropped = true;
                break;
            }
        }
        if (dropped) {
            continue;
        }

        out.append(s);
    }
    return out;
}

} // namespace kinema::core::stream_filter
