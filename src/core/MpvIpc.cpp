// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "core/MpvIpc.h"

#include "kinema_debug.h"

#include <mpv/client.h>

#include <QStringList>

#include <vector>

namespace kinema::core {

namespace {

constexpr const char* kScriptName = "kinema-overlays";

/// Parse an mpv track-id argument: integer or the literal "no".
/// Returns -1 for "no" or anything unparsable so the signal contract
/// ("-1 = disabled") stays honest.
int parseTrackId(const QString& s)
{
    if (s == QLatin1String("no")) {
        return -1;
    }
    bool ok = false;
    const int v = s.toInt(&ok);
    return ok ? v : -1;
}

} // namespace

MpvIpc::MpvIpc(mpv_handle* mpv, QObject* parent)
    : QObject(parent)
    , m_mpv(mpv)
{
}

void MpvIpc::send(const char* cmd,
    std::initializer_list<QByteArray> args)
{
    if (!m_mpv || !cmd) {
        return;
    }
    // argv layout: "script-message-to", <script>, <cmd>, args…, nullptr.
    // We keep the QByteArray arguments alive through the call via the
    // caller's lifetime (the initializer_list's backing array lives as
    // long as the enclosing full expression).
    std::vector<const char*> argv;
    argv.reserve(args.size() + 4);
    argv.push_back("script-message-to");
    argv.push_back(kScriptName);
    argv.push_back(cmd);
    for (const auto& a : args) {
        argv.push_back(a.constData());
    }
    argv.push_back(nullptr);
    mpv_command_async(m_mpv, 0, argv.data());
}

// ---- Host → script -----------------------------------------------------

void MpvIpc::setContext(const QString& title, const QString& subtitle,
    const QString& kind)
{
    send("set-context",
        { title.toUtf8(), subtitle.toUtf8(), kind.toUtf8() });
}

void MpvIpc::setMediaChips(const QByteArray& json)
{
    send("set-media-chips", { json });
}

void MpvIpc::setCheatSheetText(const QString& text)
{
    send("cheat-sheet-text", { text.toUtf8() });
}

void MpvIpc::showResume(qint64 seconds, qint64 duration)
{
    send("show-resume",
        { QByteArray::number(seconds), QByteArray::number(duration) });
}

void MpvIpc::hideResume()
{
    send("hide-resume", {});
}

void MpvIpc::showNextEpisode(const QString& title,
    const QString& subtitle, int countdownSec)
{
    send("show-next-episode",
        { title.toUtf8(), subtitle.toUtf8(),
            QByteArray::number(countdownSec) });
}

void MpvIpc::updateNextEpisodeCountdown(int seconds)
{
    send("update-next-episode", { QByteArray::number(seconds) });
}

void MpvIpc::hideNextEpisode()
{
    send("hide-next-episode", {});
}

void MpvIpc::showSkip(const QString& kind, const QString& label,
    qint64 startSec, qint64 endSec)
{
    send("show-skip",
        { kind.toUtf8(), label.toUtf8(),
            QByteArray::number(startSec),
            QByteArray::number(endSec) });
}

void MpvIpc::hideSkip()
{
    send("hide-skip", {});
}

void MpvIpc::setTracks(const QByteArray& json)
{
    send("set-tracks", { json });
}

void MpvIpc::toggleCheatSheet()
{
    send("toggle-cheatsheet", {});
}

// ---- Script → host -----------------------------------------------------

void MpvIpc::deliver(const mpv_event_client_message* msg)
{
    if (!msg || msg->num_args < 1 || !msg->args[0]) {
        return;
    }
    const QString cmd = QString::fromUtf8(msg->args[0]);
    QStringList args;
    args.reserve(msg->num_args - 1);
    for (int i = 1; i < msg->num_args; ++i) {
        args.append(msg->args[i] ? QString::fromUtf8(msg->args[i])
                                 : QString());
    }

    if (cmd == QLatin1String("resume-accepted")) {
        Q_EMIT resumeAccepted();
    } else if (cmd == QLatin1String("resume-declined")) {
        Q_EMIT resumeDeclined();
    } else if (cmd == QLatin1String("next-episode-accepted")) {
        Q_EMIT nextEpisodeAccepted();
    } else if (cmd == QLatin1String("next-episode-cancelled")) {
        Q_EMIT nextEpisodeCancelled();
    } else if (cmd == QLatin1String("skip-requested")) {
        Q_EMIT skipRequested();
    } else if (cmd == QLatin1String("pick-audio") && !args.isEmpty()) {
        Q_EMIT audioPicked(parseTrackId(args.first()));
    } else if (cmd == QLatin1String("pick-subtitle") && !args.isEmpty()) {
        Q_EMIT subtitlePicked(parseTrackId(args.first()));
    } else if (cmd == QLatin1String("pick-speed") && !args.isEmpty()) {
        bool ok = false;
        const double v = args.first().toDouble(&ok);
        if (ok) {
            Q_EMIT speedPicked(v);
        } else {
            qCWarning(KINEMA) << "MpvIpc: bad speed payload" << args.first();
        }
    } else if (cmd == QLatin1String("close-player")) {
        Q_EMIT closeRequested();
    } else if (cmd == QLatin1String("toggle-fullscreen")) {
        Q_EMIT fullscreenToggleRequested();
    } else {
        qCWarning(KINEMA)
            << "MpvIpc: unknown message" << cmd << args;
    }
}

} // namespace kinema::core

#endif // KINEMA_HAVE_LIBMPV
