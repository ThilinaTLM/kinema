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
namespace kinema::domain {
class DebridCredentialsProvider;
}

namespace kinema::api {
using namespace kinema::domain;

/**
 * Concrete `domain::Indexer` backed by Peerflix's Stremio addon.
 *
 * When no debrid provider is active, Peerflix is queried on the
 * zero-config IPFS mirror (`PeerflixSettings::baseUrl()`,
 * `https://peerflix.mov`) with no pipe segment:
 * `<baseUrl>/stream/{kind}/{id}.json`.
 *
 * When a debrid provider is active, requests go to the configurable
 * Node service (`PeerflixSettings::addonBaseUrl()`,
 * `https://addon.peerflix.mov`) with a pipe-separated path segment
 * carrying sensible debrid defaults plus the active credential:
 * `<addonBaseUrl>/debridoptions=torrentlinks,nocatalog|<provider>=<key>/stream/{kind}/{id}.json`.
 * Both hosts are user-overridable so self-hosted mirrors keep
 * working. The credentials provider pointer is optional: a null
 * pointer keeps the indexer on the zero-config host, which is what
 * tests rely on.
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
        const domain::DebridCredentialsProvider* creds,
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
    const domain::DebridCredentialsProvider* m_creds;
};

} // namespace kinema::api
