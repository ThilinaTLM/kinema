// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "services/StreamAvailabilityService.h"

#include "api/RealDebridClient.h"
#include "kinema_log_rd.h"

namespace kinema::services {

StreamAvailabilityService::StreamAvailabilityService(
    api::RealDebridClient& rd, QObject* parent)
    : QObject(parent)
    , m_rd(rd)
{
}

bool StreamAvailabilityService::isRealDebridConfigured() const
{
    return !m_rd.token().isEmpty();
}

QCoro::Task<QList<api::Stream>> StreamAvailabilityService::enrich(
    QList<api::Stream> streams)
{
    if (!isRealDebridConfigured() || streams.isEmpty()) {
        co_return streams;
    }
    QSet<QString> hashes;
    for (const auto& s : streams) {
        if (!s.infoHash.isEmpty()) {
            hashes.insert(s.infoHash.toLower());
        }
    }
    QSet<QString> cached;
    for (const auto& h : hashes) {
        try {
            const auto av = co_await m_rd.instantAvailability(h);
            if (av.cached()) {
                cached.insert(h);
            }
        } catch (const std::exception& e) {
            qCInfo(KINEMA_RD) << "RD availability check failed for" << h
                           << ":" << e.what();
            // Swallow: enrichment is best-effort. The user still
            // sees Torrentio's [RD+] tag from the discovery row.
        }
    }
    for (auto& s : streams) {
        if (cached.contains(s.infoHash.toLower())) {
            s.rdCached = true;
        }
    }
    co_return streams;
}

} // namespace kinema::services
