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
class TorrentioSettings;
class FilterSettings;
}
namespace kinema::domain {
class DebridCredentialsProvider;
}

namespace kinema::api {
using namespace kinema::domain;

/**
 * Concrete `domain::Indexer` backed by Torrentio's Stremio addon at
 * `<TorrentioSettings::baseUrl()>/<config>/stream/{kind}/{id}.json`.
 *
 * The URL path config segment (`sort=...|qualityfilter=...`) is
 * built internally from `TorrentioSettings` (sort) and
 * `FilterSettings` (server-side exclusions).
 *
 * When a debrid provider is active (`DebridCredentialsProvider::active()`
 * returns a non-empty token) the provider key is appended to the URL
 * config segment so the upstream addon can surface its debrid-cached
 * streams. The unified downloader still resolves the picked row
 * through the matching backend at play time. The credentials
 * provider pointer is optional: a null pointer keeps the indexer in
 * its pre-debrid behaviour, which is what tests rely on.
 */
class TorrentioIndexer : public domain::Indexer
{
    Q_OBJECT
public:
    TorrentioIndexer(core::HttpClient* http,
        const config::TorrentioSettings& settings,
        const config::FilterSettings& filter,
        const domain::DebridCredentialsProvider* creds,
        QObject* parent = nullptr);
    ~TorrentioIndexer() override;

    domain::IndexerKind kind() const noexcept override
    {
        return domain::IndexerKind::Torrentio;
    }
    QString displayName() const override
    {
        return QStringLiteral("Torrentio");
    }

    virtual QCoro::Task<QList<Stream>> streams(MediaKind kind,
        QString streamId) override;

private:
    core::HttpClient* m_http;
    const config::TorrentioSettings& m_settings;
    const config::FilterSettings& m_filter;
    const domain::DebridCredentialsProvider* m_creds;
};

} // namespace kinema::api
