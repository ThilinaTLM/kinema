// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QByteArray>
#include <QObject>
#include <QString>

#include <initializer_list>

struct mpv_handle;
struct mpv_event_client_message;

namespace kinema::core {

/**
 * Typed bridge between the Kinema host and the `kinema-overlays`
 * Lua script shipped under `data/kinema/mpv/scripts/`.
 *
 * Outbound: every `set*` / `show*` / `hide*` helper issues a
 * `script-message-to kinema-overlays <cmd> <args…>` through
 * `mpv_command_async`. Arguments are UTF-8 `QByteArray`s to keep
 * the wire format explicit.
 *
 * Inbound: `MpvWidget::onMpvEvents` forwards each
 * `MPV_EVENT_CLIENT_MESSAGE` to `deliver()`. This emits one of the
 * typed signals below for recognised commands; unknown commands
 * log a warning and are otherwise ignored so script/host version
 * mismatches degrade softly.
 *
 * Stateless beyond the mpv handle. Safe to destroy before the mpv
 * context — outgoing sends short-circuit when `m_mpv` is null, and
 * `deliver()` never touches the handle.
 */
class MpvIpc : public QObject
{
    Q_OBJECT
public:
    explicit MpvIpc(mpv_handle* mpv, QObject* parent = nullptr);

    // ---- Host → script --------------------------------------------------

    /// Prime the title strip. `kind` is "movie" or "series".
    void setContext(const QString& title, const QString& subtitle,
        const QString& kind);
    /// Push the media-info chip row (JSON array of short strings
    /// such as `["4K","HDR10","H.265","EAC3 5.1"]`). The Lua
    /// chrome renders one pill per entry beneath the subtitle.
    void setMediaChips(const QByteArray& json);
    /// Newline-separated pre-translated shortcut list.
    void setCheatSheetText(const QString& text);
    /// Resume prompt lifecycle.
    void showResume(qint64 seconds, qint64 duration);
    void hideResume();
    /// Next-episode banner lifecycle.
    void showNextEpisode(const QString& title,
        const QString& subtitle, int countdownSec);
    void updateNextEpisodeCountdown(int seconds);
    void hideNextEpisode();
    /// Skip-chapter pill lifecycle.
    ///
    /// `kind` is `"intro"` / `"outro"` / `"credits"`; the Lua side
    /// uses it to persist an "always skip" toggle per kind for the
    /// current session. `startSec` / `endSec` bracket the chapter
    /// range so the timeline renderer can tint the band in addition
    /// to the floating action pill. Whole seconds — sub-second
    /// precision is not needed for a skip band that is at least a
    /// few seconds wide.
    void showSkip(const QString& kind, const QString& label,
        qint64 startSec, qint64 endSec);
    void hideSkip();
    /// Feed the audio / subtitle picker lists. JSON shape is
    /// documented in `core::tracks::toIpcJson`.
    void setTracks(const QByteArray& json);
    /// Programmatic cheat-sheet toggle (mirrors the `?` binding).
    void toggleCheatSheet();

    // ---- Script → host --------------------------------------------------

    /// Called by `MpvWidget` for each incoming
    /// `MPV_EVENT_CLIENT_MESSAGE`. Emits one matching signal for
    /// known commands; warns-and-ignores unknown ones.
    void deliver(const mpv_event_client_message* msg);

Q_SIGNALS:
    void resumeAccepted();
    void resumeDeclined();
    void nextEpisodeAccepted();
    void nextEpisodeCancelled();
    void skipRequested();
    /// `aid` is an mpv track id, or -1 when the script reported
    /// `"no"` (audio disabled).
    void audioPicked(int aid);
    /// `sid` is an mpv track id, or -1 when subtitles are off.
    void subtitlePicked(int sid);
    void speedPicked(double factor);
    void closeRequested();
    void fullscreenToggleRequested();

private:
    void send(const char* cmd,
        std::initializer_list<QByteArray> args);

    mpv_handle* m_mpv = nullptr;
};

} // namespace kinema::core

#endif // KINEMA_HAVE_LIBMPV
