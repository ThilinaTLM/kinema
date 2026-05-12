// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Indexer.h"
#include "domain/Media.h"

#include <QObject>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}
namespace kinema::config {
class PeerflixSettings;
}

namespace kinema::api {
using namespace kinema::domain;

/**
 * Concrete `domain::Indexer` backed by Peerflix's Stremio addon at
 * `<PeerflixSettings::baseUrl()>/stream/{kind}/{id}.json`.
 *
 * Peerflix is zero-config: no manifest URL to paste, no opaque
 * token segment in the path, no inline filter knobs (unlike
 * Torrentio's `qualityfilter=`). Per-row filtering is handled
 * universally by `core::stream_filter::ClientFilters`.
 *
 * Response payload is the standard Stremio addon JSON, parsed by
 * the shared `api::stremio::parseStreams`. Peerflix surfaces a
 * structured `language` field that the parser propagates to
 * `Stream::language`, making the "Non-English" filter precise.
 */
class PeerflixIndexer : public domain::Indexer
{
    Q_OBJECT
public:
    PeerflixIndexer(core::HttpClient* http,
        const config::PeerflixSettings& settings,
        QObject* parent = nullptr);
    ~PeerflixIndexer() override;

    domain::IndexerKind kind() const noexcept override
    {
        return domain::IndexerKind::Peerflix;
    }
    QString displayName() const override
    {
        return QStringLiteral("Peerflix");
    }

    virtual QCoro::Task<QList<Stream>> streams(MediaKind kind,
        QString streamId) override;

private:
    core::HttpClient* m_http;
    const config::PeerflixSettings& m_settings;
};

} // namespace kinema::api
