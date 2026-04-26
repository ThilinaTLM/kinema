// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

namespace kinema::core::cheatsheet {

/**
 * Translated cheat-sheet text for the embedded player's `?` overlay,
 * pre-formatted as `"<keys>\t<action>\n"` rows. Pushed into the QML
 * chrome via `PlayerViewModel::setCheatSheetText`; the
 * `KeyboardCheatSheet.qml` overlay renders it verbatim.
 *
 * Pure. Safe to call from any thread after `KLocalizedString` is
 * initialised (i.e. after `QCoreApplication` exists).
 */
QString render();

} // namespace kinema::core::cheatsheet
