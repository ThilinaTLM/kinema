// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/DebridCredentials.h"

namespace kinema::config {
class DebridSettings;
}

namespace kinema::controllers {

class TokenController;

/**
 * Concrete `domain::DebridCredentialsProvider` backed by the
 * persistent active-provider radio (`config::DebridSettings`) and
 * the keyring-cached token strings published by `TokenController`.
 *
 * Holds raw references — both injectees outlive every indexer in
 * the service graph (constructed in `ServiceContainer`, parented to
 * the anchor that is destroyed before this resolver per the
 * declaration order in the container header).
 *
 * `active()` returns `{None, {}}` whenever the configured token is
 * empty so indexer call-sites only have to test `provider`.
 */
class DebridCredentialsResolver final
    : public domain::DebridCredentialsProvider
{
public:
    DebridCredentialsResolver(const config::DebridSettings& settings,
        const TokenController& tokens);
    ~DebridCredentialsResolver() override = default;

    domain::ActiveDebrid active() const override;

private:
    const config::DebridSettings& m_settings;
    const TokenController& m_tokens;
};

} // namespace kinema::controllers
