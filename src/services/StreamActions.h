// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QObject>

namespace kinema::core {
class PlayerLauncher;
}

namespace kinema::services {

/**
 * Single entry point for every user-initiated action on an
 * api::Stream: copy/open magnet, copy/open direct URL, play.
 *
 * UI widgets (StreamsPanel, context menus) call these slots directly.
 * The service centralises clipboard + KIO + launcher wiring so each
 * pane no longer needs five signals and MainWindow no longer needs
 * five handler slots.
 *
 * Status-bar messages flow out via statusMessage(); MainWindow\n * connects it once to its status bar.
 */
class StreamActions : public QObject
{
    Q_OBJECT
public:
    StreamActions(core::PlayerLauncher* launcher,
        QObject* parent = nullptr);

public Q_SLOTS:
    void copyMagnet(const api::Stream& stream);
    void openMagnet(const api::Stream& stream);
    void copyDirectUrl(const api::Stream& stream);
    void openDirectUrl(const api::Stream& stream);
    void play(const api::Stream& stream);

Q_SIGNALS:
    /// Status-bar message. MainWindow connects this once.
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    core::PlayerLauncher* m_launcher;
};

} // namespace kinema::services
