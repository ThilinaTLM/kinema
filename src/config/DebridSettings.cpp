// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/DebridSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "Debrid";
constexpr auto kKeyProvider = "provider";
constexpr auto kKeyRdConfigured = "realDebridConfigured";
constexpr auto kKeyAdConfigured = "allDebridConfigured";
} // namespace

DebridSettings::DebridSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

api::DebridProvider DebridSettings::activeProvider() const
{
    const auto raw = detail::read(m_config, kGroup, kKeyProvider,
        QString { QStringLiteral("none") });
    return api::providerFromString(raw);
}

void DebridSettings::setActiveProvider(api::DebridProvider p)
{
    if (activeProvider() == p) {
        return;
    }
    detail::write(m_config, kGroup, kKeyProvider,
        api::providerToString(p));
    Q_EMIT activeProviderChanged(p);
}

bool DebridSettings::realDebridConfigured() const
{
    return detail::read(m_config, kGroup, kKeyRdConfigured, false);
}

void DebridSettings::setRealDebridConfigured(bool on)
{
    if (realDebridConfigured() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyRdConfigured, on);
    Q_EMIT realDebridConfiguredChanged(on);
}

bool DebridSettings::allDebridConfigured() const
{
    return detail::read(m_config, kGroup, kKeyAdConfigured, false);
}

void DebridSettings::setAllDebridConfigured(bool on)
{
    if (allDebridConfigured() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyAdConfigured, on);
    Q_EMIT allDebridConfiguredChanged(on);
}

} // namespace kinema::config
