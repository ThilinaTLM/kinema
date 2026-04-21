// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QString>
#include <QStringList>
#include <QStringView>
#include <QUrl>

#include <optional>

namespace kinema::core::player {

/**
 * External media players Kinema knows how to launch for Real-Debrid
 * direct URLs (and, in the future, streaming torrent clients).
 *
 * The helpers in this header are **pure** — they don't touch QProcess,
 * the filesystem, or D-Bus. PlayerLauncher takes care of the impure
 * side (executable discovery, spawning, notifications).
 */
enum class Kind {
    Mpv,
    Vlc,
    Custom,
    /// In-process mpv, rendered into a Kinema-owned widget via
    /// libmpv's render API. Only "available" when the app was built
    /// with `KINEMA_HAVE_LIBMPV`; otherwise isAvailable() returns
    /// false and callers fall back to an external player.
    Embedded,
};

/// Canonical short id used in QConfig / on the command line
/// ("mpv", "vlc", "custom").
QString toString(Kind);

/// Parse a short id back into a Kind. Returns std::nullopt for unknown
/// values so callers can fall back to their own default.
std::optional<Kind> fromString(QStringView);

/// Human-readable, i18n-friendly display name ("mpv", "VLC", …).
QString displayName(Kind);

/// Base executable name (no path) — "mpv", "vlc", or empty for Custom.
QString executableName(Kind);

/**
 * One fully-resolved player invocation: the program to spawn and the
 * argv list to pass, already containing the URL in the appropriate
 * position. Used by both PlayerLauncher and the unit tests.
 */
struct Invocation {
    QString program;
    QStringList args;

    /// True if the program is non-empty (i.e. we have something to run).
    bool isValid() const { return !program.isEmpty(); }
};

/**
 * Build the Invocation for playing `url` with `kind`.
 *
 * - Mpv / Vlc: returns the executable name (no path resolution —
 *   callers who need $PATH lookup should call findExecutable() first)
 *   plus a small argv list that launches the URL cleanly.
 * - Custom: tokenises `customCommand` using shell-like whitespace
 *   splitting. If the command contains the literal token `{url}`,
 *   that token is replaced with the URL. Otherwise the URL is
 *   appended as a final argument.
 *
 * The URL is always passed as a string; no percent-decoding is
 * performed. On non-http(s) URLs this function still returns a valid
 * invocation — it's the caller's job to decide whether a direct URL
 * is playable.
 */
Invocation buildInvocation(Kind kind,
    const QUrl& url,
    const QString& customCommand = {});

/**
 * Locate `kind`'s executable on $PATH. Returns the absolute path or
 * an empty string if not found. For Custom, returns an empty string
 * (the command will be resolved at spawn time by the shell tokenizer).
 */
QString findExecutable(Kind kind);

/// True if `kind` has an executable resolvable on $PATH. For Custom,
/// returns true iff `customCommand` is non-empty.
bool isAvailable(Kind kind, const QString& customCommand = {});

} // namespace kinema::core::player
