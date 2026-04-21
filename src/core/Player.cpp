// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Player.h"

#include <KLocalizedString>

#include <QFileInfo>
#include <QStandardPaths>

namespace kinema::core::player {

namespace {

constexpr auto kMpvId = "mpv";
constexpr auto kVlcId = "vlc";
constexpr auto kCustomId = "custom";
constexpr auto kEmbeddedId = "embedded";
constexpr auto kUrlPlaceholder = "{url}";

/// Tokenise a shell-ish command line into argv. We don't need full POSIX
/// shell semantics (no variable expansion, no globbing): the input comes
/// from a settings dialog, not untrusted remote data. We do honour "…"
/// and '…' quoting so users can embed spaces in a path or argument.
QStringList tokeniseCommand(const QString& cmd)
{
    QStringList out;
    QString cur;
    enum { Normal, InSingle, InDouble } state = Normal;
    bool hasCur = false;

    for (const QChar& ch : cmd) {
        switch (state) {
        case Normal:
            if (ch == QLatin1Char('\'')) {
                state = InSingle;
                hasCur = true;
            } else if (ch == QLatin1Char('"')) {
                state = InDouble;
                hasCur = true;
            } else if (ch.isSpace()) {
                if (hasCur) {
                    out.append(cur);
                    cur.clear();
                    hasCur = false;
                }
            } else {
                cur.append(ch);
                hasCur = true;
            }
            break;
        case InSingle:
            if (ch == QLatin1Char('\'')) {
                state = Normal;
            } else {
                cur.append(ch);
            }
            break;
        case InDouble:
            if (ch == QLatin1Char('"')) {
                state = Normal;
            } else {
                cur.append(ch);
            }
            break;
        }
    }
    if (hasCur) {
        out.append(cur);
    }
    return out;
}

} // namespace

QString toString(Kind k)
{
    switch (k) {
    case Kind::Mpv:
        return QString::fromLatin1(kMpvId);
    case Kind::Vlc:
        return QString::fromLatin1(kVlcId);
    case Kind::Custom:
        return QString::fromLatin1(kCustomId);
    case Kind::Embedded:
        return QString::fromLatin1(kEmbeddedId);
    }
    return QString::fromLatin1(kMpvId);
}

std::optional<Kind> fromString(QStringView s)
{
    if (s.compare(QLatin1String(kMpvId), Qt::CaseInsensitive) == 0) {
        return Kind::Mpv;
    }
    if (s.compare(QLatin1String(kVlcId), Qt::CaseInsensitive) == 0) {
        return Kind::Vlc;
    }
    if (s.compare(QLatin1String(kCustomId), Qt::CaseInsensitive) == 0) {
        return Kind::Custom;
    }
    if (s.compare(QLatin1String(kEmbeddedId), Qt::CaseInsensitive) == 0) {
        return Kind::Embedded;
    }
    return std::nullopt;
}

QString displayName(Kind k)
{
    switch (k) {
    case Kind::Mpv:
        return i18nc("@item:inlistbox player name", "mpv");
    case Kind::Vlc:
        return i18nc("@item:inlistbox player name", "VLC");
    case Kind::Custom:
        return i18nc("@item:inlistbox player name", "Custom command");
    case Kind::Embedded:
        return i18nc("@item:inlistbox player name", "Embedded (in-app mpv)");
    }
    return QStringLiteral("mpv");
}

QString executableName(Kind k)
{
    switch (k) {
    case Kind::Mpv:
        return QStringLiteral("mpv");
    case Kind::Vlc:
        return QStringLiteral("vlc");
    case Kind::Custom:
    case Kind::Embedded:
        return QString {};
    }
    return QString {};
}

Invocation buildInvocation(Kind kind, const QUrl& url, const QString& customCommand)
{
    Invocation inv;
    const QString urlStr = url.toString();

    switch (kind) {
    case Kind::Mpv:
        inv.program = QStringLiteral("mpv");
        // `--` terminates option parsing so a stream URL that starts
        // with a dash can never be mis-parsed as a flag. `--force-window`
        // makes the video window appear immediately even while the
        // network stream is still buffering, which is what a user
        // expects from a one-click "Play".
        inv.args = {
            QStringLiteral("--force-window=immediate"),
            QStringLiteral("--"),
            urlStr,
        };
        break;

    case Kind::Vlc:
        inv.program = QStringLiteral("vlc");
        // `--play-and-exit` mirrors the mpv default: VLC quits when the
        // stream ends instead of sitting around on a blank playlist.
        // `--no-one-instance` prevents a stale VLC from swallowing the
        // URL and showing nothing.
        inv.args = {
            QStringLiteral("--play-and-exit"),
            QStringLiteral("--no-one-instance"),
            QStringLiteral("--"),
            urlStr,
        };
        break;

    case Kind::Embedded:
        // Embedded playback does not shell out — the UI layer takes
        // a separate branch and pushes the URL into an in-process
        // libmpv instance. Return an invalid Invocation so any
        // caller that tried to spawn one gets a clear no-op.
        return {};

    case Kind::Custom: {
        auto tokens = tokeniseCommand(customCommand);
        if (tokens.isEmpty()) {
            return {};
        }
        inv.program = tokens.takeFirst();
        // Replace {url} placeholder(s) in the remaining tokens. If none
        // match, append the URL as a final argument.
        bool substituted = false;
        for (auto& t : tokens) {
            if (t.contains(QLatin1String(kUrlPlaceholder))) {
                t.replace(QLatin1String(kUrlPlaceholder), urlStr);
                substituted = true;
            }
        }
        if (!substituted) {
            tokens.append(urlStr);
        }
        inv.args = tokens;
        break;
    }
    }
    return inv;
}

QString findExecutable(Kind kind)
{
    const auto name = executableName(kind);
    if (name.isEmpty()) {
        return QString {};
    }
    return QStandardPaths::findExecutable(name);
}

bool isAvailable(Kind kind, const QString& customCommand)
{
    if (kind == Kind::Embedded) {
#ifdef KINEMA_HAVE_LIBMPV
        return true;
#else
        return false;
#endif
    }
    if (kind == Kind::Custom) {
        const auto tokens = tokeniseCommand(customCommand);
        if (tokens.isEmpty()) {
            return false;
        }
        // A bare "mpv" resolves against $PATH; an absolute path must exist.
        return !QStandardPaths::findExecutable(tokens.first()).isEmpty()
            || QFileInfo::exists(tokens.first());
    }
    return !findExecutable(kind).isEmpty();
}

} // namespace kinema::core::player
