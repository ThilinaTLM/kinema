// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/PlaybackContext.h"

namespace kinema::api {

QString PlaybackKey::storageKey() const
{
    if (imdbId.isEmpty()) {
        return {};
    }
    if (kind == MediaKind::Series && season && episode) {
        return QStringLiteral("%1:%2:%3")
            .arg(imdbId)
            .arg(*season)
            .arg(*episode);
    }
    return imdbId;
}

HistoryStreamRef HistoryStreamRef::fromStream(const Stream& s)
{
    HistoryStreamRef ref;
    ref.infoHash = s.infoHash;
    ref.releaseName = s.releaseName;
    ref.resolution = s.resolution;
    ref.qualityLabel = s.qualityLabel;
    ref.sizeBytes = s.sizeBytes;
    ref.provider = s.provider;
    return ref;
}

bool HistoryStreamRef::matches(const Stream& s) const
{
    // Primary: hex infoHash. Canonical identifier when both sides
    // have one.
    if (!infoHash.isEmpty() && !s.infoHash.isEmpty()) {
        return s.infoHash.compare(infoHash, Qt::CaseInsensitive) == 0;
    }
    // Fallback: release name. Some Torrentio responses (notably a
    // subset of RD-resolved entries) omit infoHash from the JSON,
    // which would otherwise block every resume for those streams.
    // A case-insensitive release-name match is pragmatic — release
    // names embed group + source + codec + resolution, so collisions
    // within a single title's stream list are vanishingly rare.
    if (!releaseName.isEmpty() && !s.releaseName.isEmpty()) {
        return s.releaseName.compare(releaseName,
                   Qt::CaseInsensitive)
            == 0;
    }
    return false;
}

} // namespace kinema::api
