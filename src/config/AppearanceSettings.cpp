// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppearanceSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroupGeneral = "General";
constexpr auto kGroupMainWindow = "MainWindow";
constexpr auto kGroupPlayerWindow = "PlayerWindow";
constexpr auto kKeyCloseToTray = "closeToTray";
constexpr auto kKeyBrowseSplitter = "browseSplitter";
constexpr auto kKeySeriesPaneSplitter = "seriesPaneSplitter";
constexpr auto kKeyDetailSplitter = "detailSplitter";
constexpr auto kKeyShowMenuBar = "ShowMenuBar";
constexpr auto kKeyPlayerWindowGeometry = "Geometry";
} // namespace

AppearanceSettings::AppearanceSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

bool AppearanceSettings::closeToTray() const
{
    return detail::read(m_config, kGroupGeneral, kKeyCloseToTray, true);
}

void AppearanceSettings::setCloseToTray(bool on)
{
    if (closeToTray() == on) {
        return;
    }
    detail::write(m_config, kGroupGeneral, kKeyCloseToTray, on);
}

QByteArray AppearanceSettings::browseSplitterState() const
{
    return detail::read(m_config, kGroupGeneral, kKeyBrowseSplitter,
        QByteArray {});
}

void AppearanceSettings::setBrowseSplitterState(QByteArray state)
{
    detail::write(m_config, kGroupGeneral, kKeyBrowseSplitter, state);
}

QByteArray AppearanceSettings::seriesPaneSplitterState() const
{
    return detail::read(m_config, kGroupGeneral, kKeySeriesPaneSplitter,
        QByteArray {});
}

void AppearanceSettings::setSeriesPaneSplitterState(QByteArray state)
{
    detail::write(m_config, kGroupGeneral, kKeySeriesPaneSplitter, state);
}

QByteArray AppearanceSettings::detailSplitterState() const
{
    return detail::read(m_config, kGroupGeneral, kKeyDetailSplitter,
        QByteArray {});
}

void AppearanceSettings::setDetailSplitterState(QByteArray state)
{
    detail::write(m_config, kGroupGeneral, kKeyDetailSplitter, state);
}

bool AppearanceSettings::showMenuBar() const
{
    return detail::read(m_config, kGroupMainWindow, kKeyShowMenuBar, false);
}

void AppearanceSettings::setShowMenuBar(bool on)
{
    detail::write(m_config, kGroupMainWindow, kKeyShowMenuBar, on);
}

QByteArray AppearanceSettings::playerWindowGeometry() const
{
    return detail::read(m_config, kGroupPlayerWindow,
        kKeyPlayerWindowGeometry, QByteArray {});
}

void AppearanceSettings::setPlayerWindowGeometry(QByteArray geometry)
{
    detail::write(m_config, kGroupPlayerWindow,
        kKeyPlayerWindowGeometry, geometry);
}

} // namespace kinema::config
