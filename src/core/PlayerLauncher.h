// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "core/Player.h"

#include <QObject>
#include <QString>
#include <QUrl>

namespace kinema::core {

/**
 * Spawns an external media player (mpv / VLC / user-defined command) as
 * a detached process and reports success or failure via signals + a
 * KNotification.
 *
 * The launcher reads the user's preferred player and custom command
 * from Config on every call, so Settings changes take effect
 * immediately without re-wiring. If the preferred player isn't on
 * $PATH it falls back to the first available known player.
 *
 * The launcher never blocks the UI: it uses `QProcess::startDetached`,
 * so the app keeps running while playback is live. When the player
 * exits, the OS reaps the process — we deliberately don't keep a
 * handle to it.
 */
class PlayerLauncher : public QObject
{
    Q_OBJECT
public:
    explicit PlayerLauncher(QObject* parent = nullptr);
    ~PlayerLauncher() override;

    /**
     * Attempt to play `url` in the user's preferred available player.
     *
     * @param url    the stream URL to hand to the player. Must be a
     *               non-empty, absolute URL — magnet links are not
     *               accepted (mpv/VLC don't play them directly).
     * @param title  a human-readable description shown in
     *               notifications (release name or episode label).
     *               Optional; defaults to the URL itself.
     *
     * The call never blocks. On success `launched` is emitted
     * (typically within a few ms, once the fork+exec has returned);
     * on failure `launchFailed` is emitted with a human-readable
     * message. Both also surface as desktop notifications.
     */
    void play(const QUrl& url, const QString& title = {});

    /**
     * Best-effort availability check used at startup so the UI can grey
     * out the "Play" action with a tooltip explaining what to install.
     * Uses `Config::preferredPlayer()` unless `kindOverride` is set.
     */
    bool preferredPlayerAvailable() const;
    bool playerAvailable(player::Kind kind) const;

Q_SIGNALS:
    /// Emitted when the player process was spawned successfully.
    void launched(kinema::core::player::Kind kind, const QString& title);
    /// Emitted when the player could not be launched. `reason` is a
    /// localised one-line message suitable for a status bar / toast.
    void launchFailed(kinema::core::player::Kind kind, const QString& reason);

private:
    /// Resolve which player to actually use for this invocation.
    /// Returns std::nullopt if nothing is available.
    std::optional<player::Kind> resolvePlayer() const;

    void notifyLaunched(player::Kind kind, const QString& title);
    void notifyFailed(player::Kind kind, const QString& reason);
};

} // namespace kinema::core
