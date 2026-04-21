// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppearanceSettings.h"

#include <KConfigGroup>

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
    return m_config->group(QString::fromLatin1(kGroupGeneral))
        .readEntry(kKeyCloseToTray, true);
}

void AppearanceSettings::setCloseToTray(bool on)
{
    if (closeToTray() == on) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroupGeneral));
    g.writeEntry(kKeyCloseToTray, on);
    g.sync();
}

QByteArray AppearanceSettings::browseSplitterState() const
{
    return m_config->group(QString::fromLatin1(kGroupGeneral))
        .readEntry(kKeyBrowseSplitter, QByteArray {});
}

void AppearanceSettings::setBrowseSplitterState(QByteArray state)
{
    auto g = m_config->group(QString::fromLatin1(kGroupGeneral));
    g.writeEntry(kKeyBrowseSplitter, state);
    g.sync();
}

QByteArray AppearanceSettings::seriesPaneSplitterState() const
{
    return m_config->group(QString::fromLatin1(kGroupGeneral))
        .readEntry(kKeySeriesPaneSplitter, QByteArray {});
}

void AppearanceSettings::setSeriesPaneSplitterState(QByteArray state)
{
    auto g = m_config->group(QString::fromLatin1(kGroupGeneral));
    g.writeEntry(kKeySeriesPaneSplitter, state);
    g.sync();
}

QByteArray AppearanceSettings::detailSplitterState() const
{
    return m_config->group(QString::fromLatin1(kGroupGeneral))
        .readEntry(kKeyDetailSplitter, QByteArray {});
}

void AppearanceSettings::setDetailSplitterState(QByteArray state)
{
    auto g = m_config->group(QString::fromLatin1(kGroupGeneral));
    g.writeEntry(kKeyDetailSplitter, state);
    g.sync();
}

bool AppearanceSettings::showMenuBar() const
{
    return m_config->group(QString::fromLatin1(kGroupMainWindow))
        .readEntry(kKeyShowMenuBar, false);
}

void AppearanceSettings::setShowMenuBar(bool on)
{
    auto g = m_config->group(QString::fromLatin1(kGroupMainWindow));
    g.writeEntry(kKeyShowMenuBar, on);
    g.sync();
}

QByteArray AppearanceSettings::playerWindowGeometry() const
{
    return m_config->group(QString::fromLatin1(kGroupPlayerWindow))
        .readEntry(kKeyPlayerWindowGeometry, QByteArray {});
}

void AppearanceSettings::setPlayerWindowGeometry(QByteArray geometry)
{
    auto g = m_config->group(QString::fromLatin1(kGroupPlayerWindow));
    g.writeEntry(kKeyPlayerWindowGeometry, geometry);
    g.sync();
}

} // namespace kinema::config
