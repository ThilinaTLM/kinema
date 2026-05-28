// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QtGlobal>

namespace kinema::playback::transfer {

/**
 * Transient per-asset telemetry surfaced by the transfer subsystem.
 *
 * The owning component (`TransferSupervisor`) keeps the live values
 * in memory only; only the persisted `cachedBytes / state` fields
 * make it to `DownloadRepository`. View-models read these via
 * `TransferUseCase::liveStatsFor(assetId)` for the Downloads page.
 *
 * Lives under `playback/transfer/` rather than `download/` so the
 * rewired controllers / view-models don't need to keep a legacy
 * `download/` include when the unified-downloader scaffolding
 * retires (sub-commit 12).
 */
struct LiveAssetStats {
    qint64 ratePayloadBps = 0;
    int peers = 0;
    int seeds = 0;
    int etaSeconds = -1;
};

} // namespace kinema::playback::transfer
