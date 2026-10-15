// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/DebridSettingsViewModel.h"
#include "config/DebridSettings.h"
#include "domain/Debrid.h"

namespace kinema::ui::qml::settings {

// ============================== Debrid parent VM =========================

DebridSettingsViewModel::DebridSettingsViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
    , m_rd(new RealDebridSectionViewModel(http, tokens, settings, this))
    , m_ad(new AllDebridSectionViewModel(http, tokens, settings, this))
{
    connect(&m_settings, &config::DebridSettings::activeProviderChanged,
        this, [this](domain::DebridProvider) {
            Q_EMIT activeProviderChanged();
        });
}

int DebridSettingsViewModel::activeProvider() const
{
    return static_cast<int>(m_settings.activeProvider());
}

void DebridSettingsViewModel::setActiveProvider(int provider)
{
    auto value = domain::DebridProvider::None;
    switch (provider) {
    case static_cast<int>(domain::DebridProvider::RealDebrid):
        value = domain::DebridProvider::RealDebrid;
        break;
    case static_cast<int>(domain::DebridProvider::AllDebrid):
        value = domain::DebridProvider::AllDebrid;
        break;
    default:
        break;
    }
    m_settings.setActiveProvider(value);
}

} // namespace kinema::ui::qml::settings
