// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Debrid.h"
#include "domain/Download.h"
#include "domain/Media.h"

#include <QString>

#include <optional>
#include <variant>
#include <vector>

namespace kinema::playback::policy {

/// One backend's claim to handle a stream. The host caller fills in
/// the available backend list and indicates whether each one is
/// configured and capable of handling the candidate stream.
struct BackendAvailability {
    domain::DownloadBackendKind kind = domain::DownloadBackendKind::Torrent;
    bool canHandle = false;
};

struct BackendSelectionInputs {
    domain::Stream stream;
    std::vector<BackendAvailability> backends; ///< priority order
    domain::DebridProvider activeDebrid = domain::DebridProvider::None;
    std::optional<domain::DownloadBackendKind> override;
};

struct BackendSelectionError {
    enum class Kind {
        OverrideUnsupported, ///< user picked a backend that can't handle the stream
        ActiveProviderUnsupported, ///< the active debrid provider can't handle the stream
        NoBackend, ///< no registered backend can handle the stream
    };
    Kind kind = Kind::NoBackend;
    QString message;
    /// When `kind == ActiveProviderUnsupported`, the provider whose
    /// failure is being reported. Used to localise the message at
    /// the call site.
    domain::DebridProvider provider = domain::DebridProvider::None;
};

using BackendSelectionResult
    = std::variant<domain::DownloadBackendKind, BackendSelectionError>;

/**
 * Pure policy that mirrors `download::BackendSelector::select`
 * exactly. Refactored as a free function so it can be tested
 * without spinning up backend instances.
 *
 * Rules:
 *   - Explicit override: only the named backend; throws when it
 *     cannot handle the stream.
 *   - Active debrid provider is honored first.
 *   - When no debrid provider is active, debrid backends are
 *     skipped in default routing and only torrent (or any non-
 *     debrid backend) is considered.
 */
BackendSelectionResult selectBackend(const BackendSelectionInputs& in);

/// True when `kind` is a debrid HTTP backend.
bool isDebridBackend(domain::DownloadBackendKind kind) noexcept;

/// True when `kind` matches `provider`.
bool matchesActiveDebrid(domain::DownloadBackendKind kind,
    domain::DebridProvider provider) noexcept;

/// Localised display name for the provider, used in error
/// messages built by `selectBackend`.
QString providerDisplayName(domain::DebridProvider provider);

} // namespace kinema::playback::policy
