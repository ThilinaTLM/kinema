// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QList>
#include <QObject>
#include <QSet>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::api {
class RealDebridClient;
}

namespace kinema::services {

/**
 * Annotates a list of Torrentio streams with Real-Debrid availability
 * data. RD is only consulted when the user has configured a token.
 *
 * The service queries `instantAvailability` for the unique info
 * hashes in the input list and flips `Stream::rdCached` accordingly.
 * If RD is not configured, `enrich()` returns the streams unchanged.
 */
class StreamAvailabilityService : public QObject
{
    Q_OBJECT
public:
    explicit StreamAvailabilityService(api::RealDebridClient& rd,
        QObject* parent = nullptr);

    /// Returns true when the RD client carries a non-empty token.
    bool isRealDebridConfigured() const;

    /// Annotate `streams` in place with `rdCached` flags. Best-effort:
    /// network failures leave the streams unchanged. Virtual so tests
    /// can stub the entire pipeline cheaply.
    virtual QCoro::Task<QList<api::Stream>> enrich(QList<api::Stream> streams);

private:
    api::RealDebridClient& m_rd;
};

} // namespace kinema::services
