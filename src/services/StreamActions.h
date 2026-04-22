// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QObject>

namespace kinema::controllers {
class HistoryController;
}

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
 * Status-bar messages flow out via statusMessage(); MainWindow
 * connects it once to its status bar.
 */
class StreamActions : public QObject
{
    Q_OBJECT
public:
    StreamActions(core::PlayerLauncher* launcher,
        QObject* parent = nullptr);

    /// Wire the history controller used to seed resume-from and
    /// record play-start entries. Two-phase init because the
    /// controller depends on `this` via resumeFromHistory.
    void setHistoryController(controllers::HistoryController* history);

public Q_SLOTS:
    void copyMagnet(const api::Stream& stream);
    void openMagnet(const api::Stream& stream);
    void copyDirectUrl(const api::Stream& stream);
    void openDirectUrl(const api::Stream& stream);

    /// Play `stream` with the identity/title information in `ctx`.
    /// Fills `ctx.streamRef` from the stream and asks the history
    /// controller (if wired) for a resume position before handing
    /// off to PlayerLauncher.
    void play(const api::Stream& stream, const api::PlaybackContext& ctx);

Q_SIGNALS:
    /// Status-bar message. MainWindow connects this once.
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    core::PlayerLauncher* m_launcher;
    controllers::HistoryController* m_history {};
};

} // namespace kinema::services
