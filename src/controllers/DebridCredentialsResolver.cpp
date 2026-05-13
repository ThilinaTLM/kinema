// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/DebridCredentialsResolver.h"

#include "config/DebridSettings.h"
#include "controllers/TokenController.h"

namespace kinema::controllers {

DebridCredentialsResolver::DebridCredentialsResolver(
    const config::DebridSettings& settings, const TokenController& tokens)
    : m_settings(settings)
    , m_tokens(tokens)
{
}

domain::ActiveDebrid DebridCredentialsResolver::active() const
{
    switch (m_settings.activeProvider()) {
    case domain::DebridProvider::RealDebrid: {
        const auto& tok = m_tokens.realDebridToken();
        if (tok.isEmpty()) {
            return {};
        }
        return { domain::DebridProvider::RealDebrid, tok };
    }
    case domain::DebridProvider::AllDebrid: {
        const auto& key = m_tokens.allDebridApiKey();
        if (key.isEmpty()) {
            return {};
        }
        return { domain::DebridProvider::AllDebrid, key };
    }
    case domain::DebridProvider::None:
        break;
    }
    return {};
}

} // namespace kinema::controllers
