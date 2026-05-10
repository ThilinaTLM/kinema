// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/PlayerLauncher.h"

#include "config/PlayerSettings.h"
#include "kinema_log_app.h"

#include <KLocalizedString>
#include <KNotification>

#include <QProcess>

namespace kinema::core {

namespace {

constexpr auto kEventLaunched = "playbackStarted";
constexpr auto kEventFailed = "playbackFailed";

void sendNotification(const char* eventId,
    const QString& title,
    const QString& text,
    const QString& iconName)
{
    auto* n = new KNotification(QString::fromLatin1(eventId),
        KNotification::CloseOnTimeout);
    n->setTitle(title);
    n->setText(text);
    n->setIconName(iconName);
    n->sendEvent();
}

} // namespace

PlayerLauncher::PlayerLauncher(const config::PlayerSettings& settings,
    QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

PlayerLauncher::~PlayerLauncher() = default;

std::optional<player::Kind> PlayerLauncher::resolvePlayer() const
{
    const auto preferred = m_settings.preferred();
    const auto custom = m_settings.customCommand();

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
            qCInfo(KINEMA_APP) << "preferred player"
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
    return player::isAvailable(m_settings.preferred(),
        m_settings.customCommand());
}

bool PlayerLauncher::playerAvailable(player::Kind kind) const
{
    const auto custom = kind == player::Kind::Custom
        ? m_settings.customCommand()
        : QString {};
    return player::isAvailable(kind, custom);
}

void PlayerLauncher::play(const QUrl& url, const api::PlaybackContext& ctx)
{
    const auto title = ctx.title;

    const auto fail = [this](player::Kind kind, const QString& reason) {
        Q_EMIT launchFailed(kind, reason);
        notifyFailed(kind, reason);
    };

    if (!url.isValid() || url.isEmpty()) {
        qCWarning(KINEMA_APP) << "PlayerLauncher::play called with empty URL";
        fail(player::Kind::Mpv, i18nc("@info:status",
            "No playable URL available for this item."));
        return;
    }

    if (url.scheme() == QLatin1String("magnet")) {
        fail(player::Kind::Mpv, i18nc("@info:status",
            "mpv and VLC can't play magnet links directly. "
            "Use \u201cOpen magnet link\u201d or a Real-Debrid direct URL."));
        return;
    }

    const auto picked = resolvePlayer();
    if (!picked) {
        fail(player::Kind::Mpv, i18nc("@info:status",
            "No supported media player found. "
            "Install mpv or VLC, then try again."));
        return;
    }

    const auto kind = *picked;

    if (kind == player::Kind::Embedded) {
        qCInfo(KINEMA_APP) << "handing off to embedded player for"
                       << (title.isEmpty() ? url.toString() : title);
        Q_EMIT embeddedRequested(url, ctx);
        return;
    }

    const auto custom = kind == player::Kind::Custom
        ? m_settings.customCommand()
        : QString {};
    const auto inv = player::buildInvocation(kind, url, custom);
    if (!inv.isValid()) {
        fail(kind, i18nc("@info:status",
            "Could not build a launch command for %1.",
            player::displayName(kind)));
        return;
    }

    qCInfo(KINEMA_APP).noquote() << "launching" << inv.program
                             << inv.args.join(QLatin1Char(' '));

    qint64 pid = 0;
    const bool ok = QProcess::startDetached(inv.program, inv.args,
        QString {}, &pid);
    if (!ok) {
        qCWarning(KINEMA_APP) << "startDetached failed for" << inv.program;
        fail(kind, i18nc("@info:status",
            "Failed to launch %1 — is it installed and on $PATH?",
            player::displayName(kind)));
        return;
    }

    qCDebug(KINEMA_APP) << "player spawned, pid =" << pid;
    const auto display = title.isEmpty() ? url.toString() : title;
    Q_EMIT launched(kind, display);
    notifyLaunched(kind, display);
}

void PlayerLauncher::notifyLaunched(player::Kind kind, const QString& title)
{
    sendNotification(kEventLaunched,
        i18nc("@title:window notification",
            "Playing in %1", player::displayName(kind)),
        title,
        QStringLiteral("media-playback-start"));
}

void PlayerLauncher::notifyFailed(player::Kind kind, const QString& reason)
{
    sendNotification(kEventFailed,
        i18nc("@title:window notification",
            "Could not start %1", player::displayName(kind)),
        reason,
        QStringLiteral("dialog-error"));
}

} // namespace kinema::core
