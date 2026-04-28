// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/PlayerSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "Player";
constexpr auto kKeyPreferred = "preferred";
constexpr auto kKeyCustomCmd = "customCommand";
constexpr auto kKeyHwDecode = "hardwareDecoding";
constexpr auto kKeyAudioLang = "preferredAudioLang";
constexpr auto kKeySubLang = "preferredSubtitleLang";
constexpr auto kKeySkipIntro = "skipIntroChapters";
constexpr auto kKeyResumePromptThreshold = "resumePromptThresholdSec";
constexpr auto kKeyVolume = "rememberedVolume";
} // namespace

PlayerSettings::PlayerSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

core::player::Kind PlayerSettings::preferred() const
{
    const auto s = detail::read(m_config, kGroup, kKeyPreferred,
        QStringLiteral("mpv"));
    return core::player::fromString(s).value_or(core::player::Kind::Mpv);
}

void PlayerSettings::setPreferred(core::player::Kind k)
{
    if (preferred() == k) {
        return;
    }
    detail::write(m_config, kGroup, kKeyPreferred, core::player::toString(k));
    Q_EMIT preferredChanged(k);
}

QString PlayerSettings::customCommand() const
{
    return detail::read(m_config, kGroup, kKeyCustomCmd, QString {});
}

void PlayerSettings::setCustomCommand(const QString& command)
{
    detail::write(m_config, kGroup, kKeyCustomCmd, command);
}

bool PlayerSettings::hardwareDecoding() const
{
    return detail::read(m_config, kGroup, kKeyHwDecode, true);
}

void PlayerSettings::setHardwareDecoding(bool on)
{
    if (hardwareDecoding() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyHwDecode, on);
    Q_EMIT hardwareDecodingChanged(on);
}

QString PlayerSettings::preferredAudioLang() const
{
    return detail::read(m_config, kGroup, kKeyAudioLang, QString {});
}

void PlayerSettings::setPreferredAudioLang(const QString& lang)
{
    if (preferredAudioLang() == lang) {
        return;
    }
    detail::write(m_config, kGroup, kKeyAudioLang, lang);
    Q_EMIT preferredAudioLangChanged(lang);
}

QString PlayerSettings::preferredSubtitleLang() const
{
    return detail::read(m_config, kGroup, kKeySubLang, QString {});
}

void PlayerSettings::setPreferredSubtitleLang(const QString& lang)
{
    if (preferredSubtitleLang() == lang) {
        return;
    }
    detail::write(m_config, kGroup, kKeySubLang, lang);
    Q_EMIT preferredSubtitleLangChanged(lang);
}

bool PlayerSettings::skipIntroChapters() const
{
    return detail::read(m_config, kGroup, kKeySkipIntro, true);
}

void PlayerSettings::setSkipIntroChapters(bool on)
{
    detail::write(m_config, kGroup, kKeySkipIntro, on);
}

int PlayerSettings::resumePromptThresholdSec() const
{
    return detail::read(m_config, kGroup, kKeyResumePromptThreshold, 30);
}

void PlayerSettings::setResumePromptThresholdSec(int seconds)
{
    detail::write(m_config, kGroup, kKeyResumePromptThreshold,
        qMax(0, seconds));
}

double PlayerSettings::rememberedVolume() const
{
    return detail::read(m_config, kGroup, kKeyVolume, -1.0);
}

void PlayerSettings::setRememberedVolume(double percent)
{
    if (percent > 100.0) {
        percent = 100.0;
    } else if (percent < 0.0 && percent != -1.0) {
        percent = 0.0;
    }
    detail::write(m_config, kGroup, kKeyVolume, percent);
}

} // namespace kinema::config
