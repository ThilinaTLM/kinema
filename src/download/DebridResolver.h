// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"

#include <QMetaType>
#include <QObject>
#include <QString>
#include <QUrl>


#include <QCoro/QCoroTask>

namespace kinema::download {

/**
 * Outcome of a debrid provider's resolution pipeline. Returned by
 * `DebridResolver::resolve` so `HttpAssetSession` can fetch the
 * upstream URL into its sparse local file.
 */
struct ResolvedDebridLink {
    /// Hoster URL the `HttpAssetSession` will fetch from. Only valid
    /// for ~24 hours — callers should re-resolve on 401 / 403.
    QUrl downloadUrl;
    /// File size announced by the provider's unrestrict / unlock
    /// response. Authoritative upper bound for the local sparse file.
    qint64 fileSize = 0;
    /// File name announced by the provider. Used as a fallback
    /// display name.
    QString fileName;
    /// Provider-specific torrent / magnet id, kept around so callers
    /// can `cleanup()` on completion. Format is provider-defined
    /// (RD: opaque string id; AllDebrid: numeric id rendered as
    /// decimal).
    QString providerTorrentId;
};

/**
 * Abstract resolver interface implemented per debrid provider
 * (`RealDebridResolver`, `AllDebridResolver`). Lets
 * `HttpAssetSession` and the per-provider backends share the same
 * chunked-HTTP plumbing while differing only in the API workflow that
 * produces the upstream URL.
 *
 * Subclasses are expected to:
 *   1. Add the magnet to the user's account.
 *   2. Wait until the provider has the file list ready.
 *   3. Choose the best file for the asset hint.
 *   4. Tell the provider to materialise that file (if needed).
 *   5. Resolve a hoster link into a streamable URL.
 *
 * All methods may throw `core::HttpError` on failure; callers
 * surface the message in the download row's `lastError`.
 */
class DebridResolver : public QObject
{
    // Intentionally not Q_OBJECT: this is a pure-virtual interface
    // with no signals or slots. Concrete subclasses
    // (RealDebridResolver, AllDebridResolver) carry their own
    // Q_OBJECT macros.
public:
    ~DebridResolver() override = default;

    /// Run the full pipeline. Throws on failure.
    virtual QCoro::Task<ResolvedDebridLink> resolve(api::AssetRef ref) = 0;

    /// Best-effort cleanup of the provider-side torrent/magnet entry.
    /// Implementations swallow errors.
    virtual QCoro::Task<void> cleanup(QString providerTorrentId) = 0;

protected:
    using QObject::QObject;
};

} // namespace kinema::download

Q_DECLARE_METATYPE(kinema::download::ResolvedDebridLink)
