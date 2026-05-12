// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/PlayerSettingsViewModel.h"
#include "config/PlayerSettings.h"
#include "core/mpv/Player.h"

namespace kinema::ui::qml::settings {

namespace {

int playerKindToIndex(core::player::Kind kind)
{
    switch (kind) {
    case core::player::Kind::Embedded: return 0;
    case core::player::Kind::Mpv: return 1;
    case core::player::Kind::Vlc: return 2;
    case core::player::Kind::Custom: return 3;
    }
    return 1;
}

core::player::Kind indexToPlayerKind(int idx)
{
    switch (idx) {
    case 0: return core::player::Kind::Embedded;
    case 2: return core::player::Kind::Vlc;
    case 3: return core::player::Kind::Custom;
    default: return core::player::Kind::Mpv;
    }
}

} // namespace


// ============================== Player ====================================

PlayerSettingsViewModel::PlayerSettingsViewModel(
    config::PlayerSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

int PlayerSettingsViewModel::preferredPlayer() const
{
    return playerKindToIndex(m_settings.preferred());
}
QString PlayerSettingsViewModel::customCommand() const
{
    return m_settings.customCommand();
}
bool PlayerSettingsViewModel::embeddedAvailable() const
{
    return core::player::isAvailable(core::player::Kind::Embedded);
}
bool PlayerSettingsViewModel::mpvAvailable() const
{
    return core::player::isAvailable(core::player::Kind::Mpv);
}
bool PlayerSettingsViewModel::vlcAvailable() const
{
    return core::player::isAvailable(core::player::Kind::Vlc);
}
bool PlayerSettingsViewModel::hardwareDecoding() const
{
    return m_settings.hardwareDecoding();
}
QString PlayerSettingsViewModel::preferredAudioLang() const
{
    return m_settings.preferredAudioLang();
}
QString PlayerSettingsViewModel::preferredSubtitleLang() const
{
    return m_settings.preferredSubtitleLang();
}
bool PlayerSettingsViewModel::skipIntroChapters() const
{
    return m_settings.skipIntroChapters();
}
int PlayerSettingsViewModel::resumePromptThresholdSec() const
{
    return m_settings.resumePromptThresholdSec();
}

void PlayerSettingsViewModel::setPreferredPlayer(int kind)
{
    const auto next = indexToPlayerKind(kind);
    if (m_settings.preferred() == next) {
        return;
    }
    m_settings.setPreferred(next);
    Q_EMIT preferredPlayerChanged();
}
void PlayerSettingsViewModel::setCustomCommand(const QString& cmd)
{
    if (m_settings.customCommand() == cmd) {
        return;
    }
    m_settings.setCustomCommand(cmd);
    Q_EMIT customCommandChanged();
}
void PlayerSettingsViewModel::setHardwareDecoding(bool on)
{
    if (m_settings.hardwareDecoding() == on) {
        return;
    }
    m_settings.setHardwareDecoding(on);
    Q_EMIT hardwareDecodingChanged();
}
void PlayerSettingsViewModel::setPreferredAudioLang(const QString& v)
{
    if (m_settings.preferredAudioLang() == v) {
        return;
    }
    m_settings.setPreferredAudioLang(v);
    Q_EMIT preferredAudioLangChanged();
}
void PlayerSettingsViewModel::setPreferredSubtitleLang(const QString& v)
{
    if (m_settings.preferredSubtitleLang() == v) {
        return;
    }
    m_settings.setPreferredSubtitleLang(v);
    Q_EMIT preferredSubtitleLangChanged();
}
void PlayerSettingsViewModel::setSkipIntroChapters(bool on)
{
    if (m_settings.skipIntroChapters() == on) {
        return;
    }
    m_settings.setSkipIntroChapters(on);
    Q_EMIT skipIntroChaptersChanged();
}
void PlayerSettingsViewModel::setResumePromptThresholdSec(int v)
{
    if (m_settings.resumePromptThresholdSec() == v) {
        return;
    }
    m_settings.setResumePromptThresholdSec(v);
    Q_EMIT resumePromptThresholdSecChanged();
}

} // namespace kinema::ui::qml::settings
