// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/ShortcutSections.h"

#include <KLocalizedString>

#include <QVariantList>
#include <QVariantMap>

namespace kinema::core::shortcuts {

namespace {

QVariantMap row(const QString& keys, const QString& action)
{
    QVariantMap m;
    m[QStringLiteral("keys")] = keys;
    m[QStringLiteral("action")] = action;
    return m;
}

QVariantMap section(const QString& title, const QVariantList& rows)
{
    QVariantMap m;
    m[QStringLiteral("title")] = title;
    m[QStringLiteral("rows")] = rows;
    return m;
}

} // namespace

QVariantList renderSections()
{
    // Keep in sync by hand with data/kinema/mpv/input.conf and the
    // Qt-side bindings in PlayerInputs.qml. Sectioning mirrors the
    // way Plasma's System Settings groups shortcuts.
    QVariantList playback {
        row(i18nc("@label cheat sheet key", "Space / Click"),
            i18nc("@label cheat sheet action", "Play / Pause")),
        row(i18nc("@label cheat sheet key", "F / Double-click"),
            i18nc("@label cheat sheet action", "Toggle fullscreen")),
        row(i18nc("@label cheat sheet key", "[ / ]"),
            i18nc("@label cheat sheet action", "Speed \u2212 / +")),
        row(i18nc("@label cheat sheet key", "Backspace"),
            i18nc("@label cheat sheet action", "Reset speed")),
    };

    QVariantList navigation {
        row(i18nc("@label cheat sheet key", "\u2190  \u2192"),
            i18nc("@label cheat sheet action", "Seek \u00b15 s")),
        row(i18nc("@label cheat sheet key",
                "Shift+\u2190 / Shift+\u2192"),
            i18nc("@label cheat sheet action", "Seek \u00b160 s")),
        row(i18nc("@label cheat sheet key",
                "Double-click left / right"),
            i18nc("@label cheat sheet action", "Seek \u00b110 s")),
    };

    QVariantList tracks {
        row(i18nc("@label cheat sheet key", "M"),
            i18nc("@label cheat sheet action", "Mute")),
        row(i18nc("@label cheat sheet key", "\u2191 / \u2193, Wheel"),
            i18nc("@label cheat sheet action", "Volume \u2212 / +")),
        row(i18nc("@label cheat sheet key", "J"),
            i18nc("@label cheat sheet action", "Cycle subtitles")),
        row(i18nc("@label cheat sheet key", "#"),
            i18nc("@label cheat sheet action", "Cycle audio track")),
    };

    QVariantList episodes {
        row(i18nc("@label cheat sheet key", "N"),
            i18nc("@label cheat sheet action",
                "Play next episode now")),
        row(i18nc("@label cheat sheet key", "Shift+N"),
            i18nc("@label cheat sheet action", "Cancel auto-next")),
        row(i18nc("@label cheat sheet key", "S"),
            i18nc("@label cheat sheet action",
                "Skip intro / outro")),
    };

    QVariantList window {
        row(i18nc("@label cheat sheet key", "I"),
            i18nc("@label cheat sheet action", "Stats overlay")),
        row(i18nc("@label cheat sheet key", "?"),
            i18nc("@label cheat sheet action", "Player info")),
        row(i18nc("@label cheat sheet key", "Esc"),
            i18nc("@label cheat sheet action",
                "Leave fullscreen / close")),
    };

    QVariantList sections {
        section(i18nc("@title shortcut section", "Playback"),
            playback),
        section(i18nc("@title shortcut section", "Navigation"),
            navigation),
        section(i18nc("@title shortcut section", "Tracks & Audio"),
            tracks),
        section(i18nc("@title shortcut section", "Episodes & Chapters"),
            episodes),
        section(i18nc("@title shortcut section", "Window"),
            window),
    };
    return sections;
}

} // namespace kinema::core::shortcuts
