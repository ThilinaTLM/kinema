// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"

#include <QString>
#include <QVector>

namespace kinema::playback::ports {

/**
 * File catalog lookup port. Implementations (e.g. `SessionRegistry`)
 * walk active media-source sessions to produce a generic file list,
 * independent of whether it came from libtorrent or a debrid pack.
 *
 * Used by `SeriesSessionService` to compute season-pack adjacency.
 */
class SessionFileCatalog
{
public:
    virtual ~SessionFileCatalog() = default;

    /// Files for the session that matches `streamRef`. Currently
    /// keyed by `streamRef.infoHash`; returns an empty vector when
    /// no live session matches.
    virtual QVector<domain::MediaFileEntry> filesForStreamRef(
        const domain::HistoryStreamRef& streamRef) const
        = 0;

    /// Files for an explicit asset id, when known. Default returns
    /// `filesForStreamRef` with an empty ref (i.e. empty).
    virtual QVector<domain::MediaFileEntry> filesForAssetId(
        const QString& /*assetId*/) const
    {
        return {};
    }
};

} // namespace kinema::playback::ports
