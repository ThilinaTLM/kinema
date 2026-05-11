// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TorrentioIndexer.h"

#include "api/StremioStreamParse.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "core/HttpClient.h"
#include "core/TorrentioConfig.h"
#include "kinema_log_api.h"

#include <QElapsedTimer>

namespace kinema::api {

TorrentioIndexer::TorrentioIndexer(core::HttpClient* http,
    const config::TorrentioSettings& settings,
    const config::FilterSettings& filter,
    QObject* parent)
    : Indexer(parent)
    , m_http(http)
    , m_settings(settings)
    , m_filter(filter)
{
}

TorrentioIndexer::~TorrentioIndexer() = default;

QCoro::Task<QList<Stream>> TorrentioIndexer::streams(MediaKind kind,
    QString streamId)
{
    // Build per-request options from the live settings. Reading here
    // (rather than caching at ctor time) means user toggles in the
    // settings page take effect on the next fetch with no extra
    // signal wiring.
    core::torrentio::ConfigOptions opts;
    opts.sort = m_settings.defaultSort();
    opts.excludedResolutions = m_filter.excludedResolutions();
    opts.excludedCategories = m_filter.excludedCategories();

    const auto kindStr = mediaKindToPath(kind);
    const auto cfg = core::torrentio::toPathSegment(opts);

    QUrl url(m_settings.baseUrl());
    if (cfg.isEmpty()) {
        url.setPath(QStringLiteral("/stream/%1/%2.json").arg(kindStr, streamId));
    } else {
        url.setPath(
            QStringLiteral("/%1/stream/%2/%3.json").arg(cfg, kindStr, streamId));
    }

    QElapsedTimer t;
    t.start();
    const auto doc = co_await m_http->getJson(url);
    auto streams = stremio::parseStreams(doc);
    qCInfo(KINEMA_API)
        << "Torrentio:" << mediaKindToPath(kind) << streamId
        << "\u2192" << streams.size() << "streams in" << t.elapsed() << "ms";
    co_return streams;
}

} // namespace kinema::api
