// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "playback/sources/DebridResolver.h"

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::api {
class AllDebridClient;
}

namespace kinema::playback::sources {

/**
 * AllDebrid implementation of `DebridResolver`. The pipeline mirrors
 * `RealDebridResolver` but maps onto AllDebrid's API:
 *
 *   1. uploadMagnet(magnet)   -> magnet id
 *   2. magnetStatus(id)       -> poll until statusCode == 4 (Ready)
 *   3. magnetFiles(id)        -> flatten the file tree
 *   4. pick best file via the shared `picker` heuristic
 *   5. unlockLink(file)       -> direct hoster URL (handles delayed
 *                                links transparently inside the
 *                                client)
 */
class AllDebridResolver : public DebridResolver
{
    Q_OBJECT
public:
    explicit AllDebridResolver(api::AllDebridClient& ad, QObject* parent = nullptr);

    bool isConfigured() const override;
    QCoro::Task<ResolvedDebridLink> resolve(domain::AssetRef ref) override;

private:
    api::AllDebridClient& m_ad;
};

} // namespace kinema::playback::sources
