// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>
#include <QVariantList>

namespace kinema::core::shortcuts {

/**
 * Translated keyboard-shortcut sections for the embedded player's
 * info overlay. Returns a list of `QVariantMap`s shaped like:
 *
 *   [
 *     { "title": "Playback",
 *       "rows": [
 *         { "keys": "Space / Click", "action": "Play / Pause" },
 *         …
 *       ] },
 *     …
 *   ]
 *
 * Pushed into the QML chrome via
 * `PlayerViewModel::setShortcutSections`; `InfoOverlay.qml`
 * iterates with `Repeater`s and renders each section under a
 * `SectionHeader`.
 *
 * Pure. Safe to call from any thread after `KLocalizedString` is
 * initialised (i.e. after `QCoreApplication` exists).
 */
QVariantList renderSections();

} // namespace kinema::core::shortcuts
