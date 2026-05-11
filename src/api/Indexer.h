// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QMetaType>
#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::api {

/**
 * Which stream indexer the user has chosen as the active one.
 *
 * "Indexer" here is the application-layer abstraction: in practice
 * Torrentio / MediaFusion / Comet etc. are aggregators that talk to
 * real public indexers (YTS, EZTV, 1337x, …) for us, but from
 * Kinema's perspective they ARE the indexers we query. Naming
 * parallels the *arr stack (Sonarr/Radarr/Prowlarr) where users
 * pick "indexers" without caring about the layer underneath.
 *
 * Stable integer values: persisted indirectly via
 * `indexerKindToString()` into the `[Indexers]` KConfig group;
 * the strings are the canonical wire format, the ints exist only
 * so the enum survives `Q_DECLARE_METATYPE` and QML.
 */
enum class IndexerKind {
    Torrentio = 1,
    MediaFusion = 2,
};

/// Stable string token for KConfig persistence. Returns
/// `"torrentio"` / `"mediafusion"`.
QString indexerKindToString(IndexerKind);

/// Inverse of `indexerKindToString`. Unknown / empty values map to
/// `IndexerKind::Torrentio` (the default).
IndexerKind indexerKindFromString(const QString& s);

/**
 * Abstract base for everything that can answer
 * "give me candidate streams for this Stremio stream id."
 *
 * Concrete implementations (`TorrentioIndexer`, `MediaFusionIndexer`,
 * …) read their own per-indexer settings internally — the interface
 * deliberately has no `ConfigOptions` parameter because every
 * service encodes URL config differently (Torrentio uses a
 * pipe-separated path segment; MediaFusion uses an opaque base64
 * blob; future indexers may use query parameters, a POST body,
 * Bearer headers, …).
 */
class Indexer : public QObject
{
    Q_OBJECT
public:
    explicit Indexer(QObject* parent = nullptr);
    ~Indexer() override;

    virtual IndexerKind kind() const noexcept = 0;

    /// Human-readable display name for settings / status UI.
    /// Not translated — these are proper nouns (brand names).
    virtual QString displayName() const = 0;

    /**
     * Fetch candidate streams for `streamId` (IMDB id for movies,
     * `ttXXXXXX:S:E` for series episodes).
     *
     * Throws `core::HttpError` on transport / JSON failures; callers
     * catch via the existing single `catch (const std::exception&)`
     * boundary in view-models / controllers and translate with
     * `core::describeError`.
     */
    virtual QCoro::Task<QList<Stream>> streams(MediaKind kind,
        QString streamId)
        = 0;

    /**
     * Lightweight probe used by the Settings page Test buttons.
     * Default implementation runs `streams(Movie, "tt0133093")` and
     * reports success on any non-throw outcome regardless of result
     * size — "the service responded with parseable JSON" is the
     * useful signal. Subclasses can override for a cheaper probe.
     */
    virtual QCoro::Task<bool> testConnection();
};

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::IndexerKind)
