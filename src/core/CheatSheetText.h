// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

namespace kinema::core::cheatsheet {

/**
 * Translated cheat-sheet text for the embedded player's `?` overlay,
 * pre-formatted as `"<keys>\t<action>\n"` rows. Handed over to the
 * `kinema-overlays` Lua script via `MpvIpc::setCheatSheetText` so
 * i18n stays on the C++ side (Lua never holds English strings the
 * user sees).
 *
 * Pure. Safe to call from any thread after `KLocalizedString` is
 * initialised (i.e. after `QCoreApplication` exists).
 */
QString render();

} // namespace kinema::core::cheatsheet
