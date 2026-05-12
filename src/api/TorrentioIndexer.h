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

namespace kinema::api {
using namespace kinema::domain;

/**
 * Concrete `domain::Indexer` backed by Torrentio's Stremio addon at
 * `<TorrentioSettings::baseUrl()>/<config>/stream/{kind}/{id}.json`.
 *
 * The URL path config segment (`sort=...|qualityfilter=...`) is
 * built internally from `TorrentioSettings` (sort) and
 * `FilterSettings` (server-side exclusions). Real-Debrid is no
 * longer encoded into the URL — RD is handled by the unified
 * downloader after a row is picked.
 */
class TorrentioIndexer : public domain::Indexer
{
    Q_OBJECT
public:
    TorrentioIndexer(core::HttpClient* http,
        const config::TorrentioSettings& settings,
        const config::FilterSettings& filter,
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
};

} // namespace kinema::api
