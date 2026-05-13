// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TorrentioIndexer.h"

#include "api/StremioStreamParse.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "core/io/HttpClient.h"
#include "core/util/TorrentioConfig.h"
#include "core/util/TorrentioUrl.h"
#include "domain/DebridCredentials.h"
#include "kinema_log_api.h"

#include <QElapsedTimer>

namespace kinema::api {
using namespace kinema::domain;

TorrentioIndexer::TorrentioIndexer(core::HttpClient* http,
    const config::TorrentioSettings& settings,
    const config::FilterSettings& filter,
    const domain::DebridCredentialsProvider* creds,
    QObject* parent)
    : domain::Indexer(parent)
    , m_http(http)
    , m_settings(settings)
    , m_filter(filter)
    , m_creds(creds)
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

    // Forward the currently-active debrid credential, if any. The
    // resolver collapses empty-token cases to `None`, so we don't
    // need a separate guard here. Never logged — the token is
    // sensitive and the existing `qCInfo` line only carries
    // non-secret request metadata.
    if (m_creds) {
        const auto a = m_creds->active();
        opts.debridProvider = domain::providerToUrlToken(a.provider);
        opts.debridToken = a.token;
    }

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

    // When Torrentio is queried with a debrid credential, cached /
    // pending rows come back with only a `url` field pointing at
    // Torrentio's own `/resolve/<provider>/<token>/<hash>/<file>/<idx>/<file>`
    // endpoint and no structured `infoHash`. Recover the hash + file
    // index from that URL so `StreamActions` routes the row through
    // the unified downloader (which then uses Kinema's local debrid
    // resolver), and clear `directUrl` so the embedded credential
    // never escapes the process via clipboard / URL-open actions.
    for (auto& s : streams) {
        if (!s.infoHash.isEmpty() || s.directUrl.isEmpty()) {
            continue;
        }
        const auto info = core::torrentio::parseResolveUrl(s.directUrl);
        if (!info) {
            continue;
        }
        s.infoHash = info->infoHash;
        if (s.fileIndex < 0) {
            s.fileIndex = info->fileIndex;
        }
        if (s.fileNameHint.isEmpty()) {
            s.fileNameHint = info->fileNameHint;
        }
        s.directUrl.clear();
    }

    qCInfo(KINEMA_API)
        << "Torrentio:" << mediaKindToPath(kind) << streamId
        << "\u2192" << streams.size() << "streams in" << t.elapsed() << "ms";
    co_return streams;
}

} // namespace kinema::api
