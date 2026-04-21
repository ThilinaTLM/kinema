// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/RealDebridSettings.h"

#include <KConfigGroup>

namespace kinema::config {

namespace {
constexpr auto kGroup = "RealDebrid";
constexpr auto kKey = "configured";
} // namespace

RealDebridSettings::RealDebridSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

bool RealDebridSettings::configured() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKey, false);
}

void RealDebridSettings::setConfigured(bool on)
{
    if (configured() == on) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKey, on);
    g.sync();
    Q_EMIT configuredChanged(on);
}

} // namespace kinema::config
