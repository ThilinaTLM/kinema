// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/RealDebridSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "RealDebrid";
constexpr auto kKeyConfigured = "configured";
constexpr auto kKeyEnabled = "enabled";
} // namespace

RealDebridSettings::RealDebridSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

bool RealDebridSettings::configured() const
{
    return detail::read(m_config, kGroup, kKeyConfigured, false);
}

void RealDebridSettings::setConfigured(bool on)
{
    if (configured() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyConfigured, on);
    Q_EMIT configuredChanged(on);
}

bool RealDebridSettings::enabled() const
{
    return detail::read(m_config, kGroup, kKeyEnabled, true);
}

void RealDebridSettings::setEnabled(bool on)
{
    if (enabled() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyEnabled, on);
    Q_EMIT enabledChanged(on);
}

} // namespace kinema::config
