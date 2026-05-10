// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

namespace kinema::core {

/**
 * Installs a process-wide rich message pattern, a chaining
 * `qInstallMessageHandler` that fans out to stderr (colorised on TTY)
 * and an optional rotating file under `AppDataLocation/logs/`, and
 * writes a one-line session header.
 *
 * Idempotent: subsequent calls are no-ops. Safe to invoke before
 * `QApplication` construction (uses no event loop / queued slots).
 *
 * Honours `QT_MESSAGE_PATTERN` (Qt's default override) and
 * `QStandardPaths::isTestModeEnabled()` (skips file output during
 * tests).
 */
void installLogging();

} // namespace kinema::core
