// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/PlayerSettings.h"

#include <KConfigGroup>

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

bool PlayerSettings::hardwareDecoding() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyHwDecode, true);
}

void PlayerSettings::setHardwareDecoding(bool on)
{
    if (hardwareDecoding() == on) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyHwDecode, on);
    g.sync();
    Q_EMIT hardwareDecodingChanged(on);
}

QString PlayerSettings::preferredAudioLang() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyAudioLang, QString {});
}

void PlayerSettings::setPreferredAudioLang(const QString& lang)
{
    if (preferredAudioLang() == lang) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyAudioLang, lang);
    g.sync();
    Q_EMIT preferredAudioLangChanged(lang);
}

QString PlayerSettings::preferredSubtitleLang() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeySubLang, QString {});
}

void PlayerSettings::setPreferredSubtitleLang(const QString& lang)
{
    if (preferredSubtitleLang() == lang) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeySubLang, lang);
    g.sync();
    Q_EMIT preferredSubtitleLangChanged(lang);
}

bool PlayerSettings::skipIntroChapters() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeySkipIntro, true);
}

void PlayerSettings::setSkipIntroChapters(bool on)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeySkipIntro, on);
    g.sync();
}

int PlayerSettings::resumePromptThresholdSec() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyResumePromptThreshold, 30);
}

void PlayerSettings::setResumePromptThresholdSec(int seconds)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyResumePromptThreshold, qMax(0, seconds));
    g.sync();
}

double PlayerSettings::rememberedVolume() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyVolume, -1.0);
}

void PlayerSettings::setRememberedVolume(double percent)
{
    // Clamp to [-1, 100]: -1 is the "never set" sentinel, anything
    // above 100 would only surprise mpv on next load.
    if (percent > 100.0) {
        percent = 100.0;
    } else if (percent < 0.0 && percent != -1.0) {
        percent = 0.0;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyVolume, percent);
    g.sync();
}

} // namespace kinema::config
