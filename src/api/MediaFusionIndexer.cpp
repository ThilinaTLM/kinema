// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/MediaFusionIndexer.h"

#include "api/MediaFusionParse.h"
#include "api/StremioStreamParse.h"
#include "config/MediaFusionSettings.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "kinema_log_api.h"

#include <KLocalizedString>

#include <QElapsedTimer>

namespace kinema::api {

MediaFusionIndexer::MediaFusionIndexer(core::HttpClient* http,
    const config::MediaFusionSettings& settings,
    QObject* parent)
    : Indexer(parent)
    , m_http(http)
    , m_settings(settings)
{
}

MediaFusionIndexer::~MediaFusionIndexer() = default;

QCoro::Task<QList<Stream>> MediaFusionIndexer::streams(MediaKind kind,
    QString streamId)
{
    // Pre-flight: MediaFusion (at least the public ElfHosted host)
    // refuses unconfigured stream calls with HTTP 403. Require the
    // user to have saved a manifest URL first; surface a clear
    // actionable message instead of a generic transport error.
    if (m_settings.manifestUrl().isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::HttpStatus, 0,
            i18nc("@info indexer error",
                "MediaFusion isn\u2019t configured yet. Open Settings \u2192 "
                "Indexers \u2192 MediaFusion and paste a manifest URL from "
                "mediafusion.elfhosted.com/app/configure (or your own "
                "self-hosted instance)."));
    }

    // Reconstitute a ManifestUrlParts view over the live settings.
    // We don't re-parse the raw manifestUrl each call because saving
    // already split it; we read baseUrl / encryptedConfig directly.
    mediafusion::ManifestUrlParts parts;
    parts.valid = true;
    parts.baseUrl = m_settings.baseUrl();
    parts.encryptedConfig = m_settings.encryptedConfig();

    const auto kindStr = mediaKindToPath(kind);
    const QUrl url(mediafusion::buildStreamUrl(parts, kindStr, streamId));

    QElapsedTimer t;
    t.start();
    const auto doc = co_await m_http->getJson(url);
    auto streams = stremio::parseStreams(doc);
    qCInfo(KINEMA_API)
        << "MediaFusion:" << kindStr << streamId
        << "\u2192" << streams.size() << "streams in" << t.elapsed() << "ms";
    co_return streams;
}

} // namespace kinema::api
