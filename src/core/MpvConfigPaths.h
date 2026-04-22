// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

namespace kinema::core::mpv_config {

/**
 * Absolute path to the directory containing Kinema's shipped mpv
 * configs (`mpv.conf` and `input.conf`).
 *
 * Resolution order:
 *   1. `QStandardPaths::locate(GenericDataLocation, "kinema/mpv",
 *      LocateDirectory)` — this finds installed copies under
 *      $XDG_DATA_DIRS/kinema/mpv/ and the per-user override under
 *      $XDG_DATA_HOME/kinema/mpv/ (packagers / distros / install.sh).
 *   2. A build-tree fallback (compile-time `KINEMA_BUILD_DATA_DIR`
 *      baked in from CMake) so `./build/bin/kinema` finds the files
 *      during development without an install step.
 *
 * Returns an empty string when neither location resolves. Callers
 * (MpvWidget) log a warning and fall back to hard-coded defaults.
 *
 * Pure — the only side effect is reading the filesystem, which is
 * safe to call at any thread.
 */
QString dataDir();

/// Absolute path to the shipped `mpv.conf`, or empty when missing.
QString mpvConfPath();

/// Absolute path to the shipped `input.conf`, or empty when missing.
QString inputConfPath();

} // namespace kinema::core::mpv_config
