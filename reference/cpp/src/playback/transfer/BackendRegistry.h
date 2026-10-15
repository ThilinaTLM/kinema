// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Debrid.h"
#include "domain/Download.h"
#include "domain/Media.h"
#include "playback/policy/BackendSelectionPolicy.h"
#include "playback/ports/MediaSourcePort.h"

#include <memory>
#include <optional>
#include <variant>
#include <vector>

namespace kinema::playback::transfer {

/**
 * Owns `MediaSourcePort` implementations and applies the pure
 * `playback::policy::selectBackend` rule to pick one for a given
 * stream.
 *
 * Replaces `download::BackendSelector`. Backends are registered in
 * priority order; debrid backends should be registered before the
 * torrent backend. The registry forwards the active-debrid-provider
 * gate into the policy so swapping providers is a single setter
 * call rather than re-registration.
 */
class BackendRegistry
{
public:
    /// `select(...)` returns either the chosen source (non-owning
    /// pointer; lifetime tied to the registry) or the policy's
    /// rejection reason. Callers should localise the error
    /// `message` at their boundary if it has not been pre-localised
    /// by the policy.
    using SelectionResult = std::variant<ports::MediaSourcePort*,
        policy::BackendSelectionError>;

    BackendRegistry();
    ~BackendRegistry();

    BackendRegistry(const BackendRegistry&) = delete;
    BackendRegistry& operator=(const BackendRegistry&) = delete;

    /// Insert a source. The first source registered for a given
    /// `kind()` wins lookups; duplicate kinds are rejected.
    void registerSource(std::unique_ptr<ports::MediaSourcePort> source);

    void setActiveDebridProvider(domain::DebridProvider p) noexcept
    {
        m_activeDebrid = p;
    }
    domain::DebridProvider activeDebridProvider() const noexcept
    {
        return m_activeDebrid;
    }

    /// Look up a previously-registered source by its kind, or
    /// nullptr.
    ports::MediaSourcePort* find(
        domain::DownloadBackendKind kind) const;

    /// Pick a source for `stream`, optionally constrained by
    /// `override`. The returned pointer is non-owning and only
    /// valid for the registry's lifetime.
    SelectionResult select(const domain::Stream& stream,
        std::optional<domain::DownloadBackendKind> override
            = std::nullopt) const;

    int size() const noexcept
    {
        return static_cast<int>(m_sources.size());
    }

private:
    std::vector<std::unique_ptr<ports::MediaSourcePort>> m_sources;
    domain::DebridProvider m_activeDebrid = domain::DebridProvider::None;
};

} // namespace kinema::playback::transfer
