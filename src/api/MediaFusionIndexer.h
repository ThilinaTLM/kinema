// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Indexer.h"
#include "api/Media.h"

#include <QObject>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}
namespace kinema::config {
class MediaFusionSettings;
}

namespace kinema::api {

/**
 * Concrete `Indexer` backed by MediaFusion's Stremio addon at
 * `<MediaFusionSettings::baseUrl()>/[<token>/]stream/{kind}/{id}.json`.
 *
 * MediaFusion's URL config is an opaque base64 blob ("encrypted
 * config") the user gets from MediaFusion's own `/configure` web
 * UI. We treat it as a black box and just thread it through the
 * stream URL when present. Unconfigured instances respond at
 * `<baseUrl>/stream/...` with a default smaller pool, which is
 * still useful as a discovery probe.
 *
 * Response payload is the standard Stremio addon JSON, parsed by
 * the shared `api::stremio::parseStreams`.
 */
class MediaFusionIndexer : public Indexer
{
    Q_OBJECT
public:
    MediaFusionIndexer(core::HttpClient* http,
        const config::MediaFusionSettings& settings,
        QObject* parent = nullptr);
    ~MediaFusionIndexer() override;

    IndexerKind kind() const noexcept override
    {
        return IndexerKind::MediaFusion;
    }
    QString displayName() const override
    {
        return QStringLiteral("MediaFusion");
    }

    virtual QCoro::Task<QList<Stream>> streams(MediaKind kind,
        QString streamId) override;

private:
    core::HttpClient* m_http;
    const config::MediaFusionSettings& m_settings;
};

} // namespace kinema::api
