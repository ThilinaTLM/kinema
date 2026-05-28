// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/ports/ByteRangeSource.h"

#include <QString>
#include <QUrl>

namespace kinema::playback::ports {

/**
 * Abstract local HTTP stream gateway. `LocalHttpStreamGateway`
 * implements this in practice; the port exists so tests and
 * projections can swap in fakes.
 */
class StreamGatewayPort
{
public:
    virtual ~StreamGatewayPort() = default;

    virtual bool listen() = 0;
    virtual bool isListening() const = 0;

    virtual QUrl expose(ByteRangeSource& source) = 0;
    virtual QUrl urlFor(const QString& assetId) const = 0;
    virtual void revoke(ByteRangeSource& source) = 0;
    virtual void revoke(const QString& assetId) = 0;
};

} // namespace kinema::playback::ports
