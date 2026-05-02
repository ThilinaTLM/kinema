// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/LibrarySettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {

constexpr auto kGroup = "Library";
constexpr auto kKeySmartMode = "smartMode";

} // namespace

LibrarySettings::LibrarySettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

bool LibrarySettings::smartMode() const
{
    return detail::read(m_config, kGroup, kKeySmartMode, true);
}

void LibrarySettings::setSmartMode(bool on)
{
    detail::write(m_config, kGroup, kKeySmartMode, on);
}

} // namespace kinema::config
