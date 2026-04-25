// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/CheatSheetText.h"

#include <KLocalizedString>

namespace kinema::core::cheatsheet {

QString render()
{
    // Keep in sync by hand with data/kinema/mpv/input.conf + the
    // Lua script's own keybindings (?, I, F, N, Shift+N, Esc).
    // Each entry is "<keys>\t<action>\n"; the Lua renderer splits
    // on \n and then on \t for columns.
    struct Row {
        QString keys;
        QString action;
    };
    const QList<Row> rows = {
        { i18nc("@label cheat sheet key", "Space / Click"),
          i18nc("@label cheat sheet action", "Play / Pause") },
        { i18nc("@label cheat sheet key", "\u2190  \u2192"),
          i18nc("@label cheat sheet action", "Seek \u00b15 s") },
        { i18nc("@label cheat sheet key",
              "Shift+\u2190 / Shift+\u2192"),
          i18nc("@label cheat sheet action", "Seek \u00b160 s") },
        { i18nc("@label cheat sheet key", "M"),
          i18nc("@label cheat sheet action", "Mute") },
        { i18nc("@label cheat sheet key", "J"),
          i18nc("@label cheat sheet action", "Cycle subtitles") },
        { i18nc("@label cheat sheet key", "#"),
          i18nc("@label cheat sheet action", "Cycle audio track") },
        { i18nc("@label cheat sheet key", "[ / ]"),
          i18nc("@label cheat sheet action", "Speed \u2212 / +") },
        { i18nc("@label cheat sheet key", "Backspace"),
          i18nc("@label cheat sheet action", "Reset speed") },
        { i18nc("@label cheat sheet key", "F / Double-click"),
          i18nc("@label cheat sheet action", "Toggle fullscreen") },
        { i18nc("@label cheat sheet key", "Esc"),
          i18nc("@label cheat sheet action",
              "Leave fullscreen / close") },
        { i18nc("@label cheat sheet key", "I"),
          i18nc("@label cheat sheet action", "Stats overlay") },
        { i18nc("@label cheat sheet key", "N"),
          i18nc("@label cheat sheet action",
              "Play next episode now") },
        { i18nc("@label cheat sheet key", "Shift+N"),
          i18nc("@label cheat sheet action", "Cancel auto-next") },
        { i18nc("@label cheat sheet key", "S"),
          i18nc("@label cheat sheet action",
              "Skip intro / outro") },
        { i18nc("@label cheat sheet key",
              "Double-click left / right"),
          i18nc("@label cheat sheet action",
              "Seek \u00b110 s") },
        { i18nc("@label cheat sheet key", "?"),
          i18nc("@label cheat sheet action", "This help") },
    };

    QString out;
    out.reserve(rows.size() * 32);
    for (const auto& r : rows) {
        out += r.keys;
        out += QLatin1Char('\t');
        out += r.action;
        out += QLatin1Char('\n');
    }
    return out;
}

} // namespace kinema::core::cheatsheet
