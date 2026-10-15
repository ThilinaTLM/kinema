// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>

namespace kinema::config {
class PlayerSettings;
}

namespace kinema::ui::qml::settings {

class PlayerSettingsViewModel : public QObject
{
    Q_OBJECT
    /// 0 = Embedded, 1 = Mpv, 2 = Vlc, 3 = Custom.
    Q_PROPERTY(int preferredPlayer READ preferredPlayer
        WRITE setPreferredPlayer NOTIFY preferredPlayerChanged)
    Q_PROPERTY(QString customCommand READ customCommand
        WRITE setCustomCommand NOTIFY customCommandChanged)
    Q_PROPERTY(bool embeddedAvailable READ embeddedAvailable CONSTANT)
    Q_PROPERTY(bool mpvAvailable READ mpvAvailable CONSTANT)
    Q_PROPERTY(bool vlcAvailable READ vlcAvailable CONSTANT)

    Q_PROPERTY(bool hardwareDecoding READ hardwareDecoding
        WRITE setHardwareDecoding NOTIFY hardwareDecodingChanged)
    Q_PROPERTY(QString preferredAudioLang READ preferredAudioLang
        WRITE setPreferredAudioLang NOTIFY preferredAudioLangChanged)
    Q_PROPERTY(QString preferredSubtitleLang READ preferredSubtitleLang
        WRITE setPreferredSubtitleLang NOTIFY preferredSubtitleLangChanged)

    Q_PROPERTY(bool skipIntroChapters READ skipIntroChapters
        WRITE setSkipIntroChapters NOTIFY skipIntroChaptersChanged)
    Q_PROPERTY(int resumePromptThresholdSec READ resumePromptThresholdSec
        WRITE setResumePromptThresholdSec NOTIFY resumePromptThresholdSecChanged)

public:
    PlayerSettingsViewModel(config::PlayerSettings& settings,
        QObject* parent = nullptr);

    int preferredPlayer() const;
    QString customCommand() const;
    bool embeddedAvailable() const;
    bool mpvAvailable() const;
    bool vlcAvailable() const;
    bool hardwareDecoding() const;
    QString preferredAudioLang() const;
    QString preferredSubtitleLang() const;
    bool skipIntroChapters() const;
    int resumePromptThresholdSec() const;

    void setPreferredPlayer(int kind);
    void setCustomCommand(const QString& cmd);
    void setHardwareDecoding(bool on);
    void setPreferredAudioLang(const QString& v);
    void setPreferredSubtitleLang(const QString& v);
    void setSkipIntroChapters(bool on);
    void setResumePromptThresholdSec(int v);

Q_SIGNALS:
    void preferredPlayerChanged();
    void customCommandChanged();
    void hardwareDecodingChanged();
    void preferredAudioLangChanged();
    void preferredSubtitleLangChanged();
    void skipIntroChaptersChanged();
    void resumePromptThresholdSecChanged();

private:
    config::PlayerSettings& m_settings;
};

} // namespace kinema::ui::qml::settings
