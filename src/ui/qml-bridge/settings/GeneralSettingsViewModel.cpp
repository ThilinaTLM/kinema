// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/GeneralSettingsViewModel.h"
#include "config/AppearanceSettings.h"
#include "config/SearchSettings.h"
#include "domain/Media.h"
#include <QSystemTrayIcon>

namespace kinema::ui::qml::settings {

// ============================== Application ===============================

GeneralSettingsViewModel::GeneralSettingsViewModel(
    config::SearchSettings& search,
    config::AppearanceSettings& appearance, QObject* parent)
    : QObject(parent)
    , m_search(search)
    , m_appearance(appearance)
{
}

int GeneralSettingsViewModel::defaultSearchKind() const
{
    return m_search.kind() == domain::MediaKind::Series ? 1 : 0;
}

bool GeneralSettingsViewModel::closeToTray() const
{
    return m_appearance.closeToTray();
}

bool GeneralSettingsViewModel::trayAvailable() const
{
    return QSystemTrayIcon::isSystemTrayAvailable();
}

void GeneralSettingsViewModel::setDefaultSearchKind(int kind)
{
    const auto next = kind == 1 ? domain::MediaKind::Series : domain::MediaKind::Movie;
    if (m_search.kind() == next) {
        return;
    }
    m_search.setKind(next);
    Q_EMIT defaultSearchKindChanged();
}

void GeneralSettingsViewModel::setCloseToTray(bool on)
{
    if (m_appearance.closeToTray() == on) {
        return;
    }
    m_appearance.setCloseToTray(on);
    Q_EMIT closeToTrayChanged();
}

} // namespace kinema::ui::qml::settings
