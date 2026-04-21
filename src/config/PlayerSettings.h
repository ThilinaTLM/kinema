// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/Player.h"

#include <KSharedConfig>

#include <QObject>
#include <QString>

namespace kinema::config {

/**
 * External media player selection.
 *
 * KConfig group: [Player]
 * Keys:
 *   preferred      "mpv" | "vlc" | "custom" | "embedded"
 *   customCommand  free-form command line (may contain "{url}")
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

Q_SIGNALS:
    void preferredChanged(core::player::Kind);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
