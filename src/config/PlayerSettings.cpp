// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/PlayerSettings.h"

#include <KConfigGroup>

namespace kinema::config {

namespace {
constexpr auto kGroup = "Player";
constexpr auto kKeyPreferred = "preferred";
constexpr auto kKeyCustomCmd = "customCommand";
} // namespace

PlayerSettings::PlayerSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

core::player::Kind PlayerSettings::preferred() const
{
    const auto s = m_config->group(QString::fromLatin1(kGroup))
                       .readEntry(kKeyPreferred, QStringLiteral("mpv"));
    return core::player::fromString(s).value_or(core::player::Kind::Mpv);
}

void PlayerSettings::setPreferred(core::player::Kind k)
{
    if (preferred() == k) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyPreferred, core::player::toString(k));
    g.sync();
    Q_EMIT preferredChanged(k);
}

QString PlayerSettings::customCommand() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyCustomCmd, QString {});
}

void PlayerSettings::setCustomCommand(const QString& command)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyCustomCmd, command);
    g.sync();
}

} // namespace kinema::config
