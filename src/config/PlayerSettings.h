// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/Player.h"

#include <KSharedConfig>

#include <QObject>
#include <QString>

namespace kinema::config {

/**
 * Media player selection and embedded-player preferences.
 *
 * KConfig group: [Player]
 * Keys (all stable — never rename without a migration):
 *   preferred              "mpv" | "vlc" | "custom" | "embedded"
 *   customCommand          free-form command line (may contain "{url}")
 *   hardwareDecoding       bool (default true). Off → mpv hwdec=no.
 *   preferredAudioLang     ISO-639 list forwarded to mpv `alang`.
 *   preferredSubtitleLang  ISO-639 list forwarded to mpv `slang`.
 *   rememberedVolume       double in [0, 100]; -1 = "never set".
 *                          Written from PlayerWindow::hideEvent, read
 *                          back on next load. Not live-editable, so no
 *                          signal.
 */
class PlayerSettings : public QObject
{
    Q_OBJECT
public:
    explicit PlayerSettings(KSharedConfigPtr config, QObject* parent = nullptr);

    core::player::Kind preferred() const;
    void setPreferred(core::player::Kind);

    QString customCommand() const;
    void setCustomCommand(const QString& command);

    // ---- Embedded-player preferences ------------------------------------

    bool hardwareDecoding() const;
    void setHardwareDecoding(bool on);

    QString preferredAudioLang() const;
    void setPreferredAudioLang(const QString& lang);

    QString preferredSubtitleLang() const;
    void setPreferredSubtitleLang(const QString& lang);

    /// -1.0 sentinel means "never initialised"; callers should leave
    /// mpv's default (100) in that case.
    double rememberedVolume() const;
    void setRememberedVolume(double percent);

Q_SIGNALS:
    void preferredChanged(core::player::Kind);
    void hardwareDecodingChanged(bool);
    void preferredAudioLangChanged(const QString&);
    void preferredSubtitleLangChanged(const QString&);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
