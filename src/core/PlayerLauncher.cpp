// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/PlayerLauncher.h"

#include "config/Config.h"
#include "kinema_debug.h"

#include <KLocalizedString>
#include <KNotification>

#include <QProcess>

namespace kinema::core {

namespace {

constexpr auto kEventLaunched = "playbackStarted";
constexpr auto kEventFailed = "playbackFailed";

} // namespace

PlayerLauncher::PlayerLauncher(QObject* parent)
    : QObject(parent)
{
}

PlayerLauncher::~PlayerLauncher() = default;

std::optional<player::Kind> PlayerLauncher::resolvePlayer() const
{
    const auto& cfg = config::Config::instance();
    const auto preferred = cfg.preferredPlayer();
    const auto custom = cfg.customPlayerCommand();

    if (player::isAvailable(preferred, custom)) {
        return preferred;
    }

    // Fall back to the other known external players (mpv first —
    // it's our default). Embedded is excluded from the ladder: if it
    // was the preferred choice but isn't available (e.g. built
    // without libmpv), we want the user to land on mpv/VLC rather
    // than silently flipping back to the embedded path that doesn't
    // exist.
    for (auto k : { player::Kind::Mpv, player::Kind::Vlc }) {
        if (k == preferred) {
            continue;
        }
        if (player::isAvailable(k)) {
            qCInfo(KINEMA) << "preferred player"
                           << player::toString(preferred)
                           << "is unavailable; falling back to"
                           << player::toString(k);
            return k;
        }
    }
    return std::nullopt;
}

bool PlayerLauncher::preferredPlayerAvailable() const
{
    const auto& cfg = config::Config::instance();
    return player::isAvailable(cfg.preferredPlayer(), cfg.customPlayerCommand());
}

bool PlayerLauncher::playerAvailable(player::Kind kind) const
{
    const auto& cfg = config::Config::instance();
    const auto custom = kind == player::Kind::Custom ? cfg.customPlayerCommand() : QString {};
    return player::isAvailable(kind, custom);
}

void PlayerLauncher::play(const QUrl& url, const QString& title)
{
    if (!url.isValid() || url.isEmpty()) {
        const auto reason = i18nc("@info:status",
            "No playable URL available for this item.");
        qCWarning(KINEMA) << "PlayerLauncher::play called with empty URL";
        Q_EMIT launchFailed(player::Kind::Mpv, reason);
        notifyFailed(player::Kind::Mpv, reason);
        return;
    }

    // mpv / VLC don't natively play magnet URIs — catch that early so
    // the user sees a useful message instead of a failed process.
    if (url.scheme() == QLatin1String("magnet")) {
        const auto reason = i18nc("@info:status",
            "mpv and VLC can't play magnet links directly. "
            "Use \u201cOpen magnet link\u201d or a Real-Debrid direct URL.");
        Q_EMIT launchFailed(player::Kind::Mpv, reason);
        notifyFailed(player::Kind::Mpv, reason);
        return;
    }

    const auto picked = resolvePlayer();
    if (!picked) {
        const auto reason = i18nc("@info:status",
            "No supported media player found. "
            "Install mpv or VLC, then try again.");
        Q_EMIT launchFailed(player::Kind::Mpv, reason);
        notifyFailed(player::Kind::Mpv, reason);
        return;
    }

    const auto kind = *picked;

    // Embedded playback bypasses QProcess entirely — the UI layer
    // owns the PlayerWindow and fires its own notification once the
    // file has loaded.
    if (kind == player::Kind::Embedded) {
        qCInfo(KINEMA) << "handing off to embedded player for"
                       << (title.isEmpty() ? url.toString() : title);
        Q_EMIT embeddedRequested(url, title);
        return;
    }

    const auto& cfg = config::Config::instance();
    const auto custom = kind == player::Kind::Custom ? cfg.customPlayerCommand() : QString {};
    const auto inv = player::buildInvocation(kind, url, custom);
    if (!inv.isValid()) {
        const auto reason = i18nc("@info:status",
            "Could not build a launch command for %1.",
            player::displayName(kind));
        Q_EMIT launchFailed(kind, reason);
        notifyFailed(kind, reason);
        return;
    }

    qCInfo(KINEMA).noquote() << "launching" << inv.program
                             << inv.args.join(QLatin1Char(' '));

    qint64 pid = 0;
    const bool ok = QProcess::startDetached(inv.program, inv.args,
        /*workingDirectory=*/QString {}, &pid);
    if (!ok) {
        const auto reason = i18nc("@info:status",
            "Failed to launch %1 — is it installed and on $PATH?",
            player::displayName(kind));
        qCWarning(KINEMA) << "startDetached failed for" << inv.program;
        Q_EMIT launchFailed(kind, reason);
        notifyFailed(kind, reason);
        return;
    }

    qCDebug(KINEMA) << "player spawned, pid =" << pid;
    const auto display = title.isEmpty() ? url.toString() : title;
    Q_EMIT launched(kind, display);
    notifyLaunched(kind, display);
}

void PlayerLauncher::notifyLaunched(player::Kind kind, const QString& title)
{
    auto* n = new KNotification(QString::fromLatin1(kEventLaunched),
        KNotification::CloseOnTimeout);
    n->setTitle(i18nc("@title:window notification",
        "Playing in %1", player::displayName(kind)));
    n->setText(title);
    n->setIconName(QStringLiteral("media-playback-start"));
    n->sendEvent();
}

void PlayerLauncher::notifyFailed(player::Kind kind, const QString& reason)
{
    auto* n = new KNotification(QString::fromLatin1(kEventFailed),
        KNotification::CloseOnTimeout);
    n->setTitle(i18nc("@title:window notification",
        "Could not start %1", player::displayName(kind)));
    n->setText(reason);
    n->setIconName(QStringLiteral("dialog-error"));
    n->sendEvent();
}

} // namespace kinema::core
