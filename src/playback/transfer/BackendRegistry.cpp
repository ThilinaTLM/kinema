// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/transfer/BackendRegistry.h"

#include "kinema_log_download.h"

#include <algorithm>

namespace kinema::playback::transfer {

BackendRegistry::BackendRegistry() = default;
BackendRegistry::~BackendRegistry() = default;

void BackendRegistry::registerSource(
    std::unique_ptr<ports::MediaSourcePort> source)
{
    Q_ASSERT(source);
    if (!source) {
        return;
    }
    const auto kind = source->kind();
    if (find(kind) != nullptr) {
        qCWarning(KINEMA_DOWNLOAD)
            << "BackendRegistry::registerSource: duplicate kind"
            << static_cast<int>(kind) << "ignored";
        return;
    }
    m_sources.push_back(std::move(source));
}

ports::MediaSourcePort* BackendRegistry::find(
    domain::DownloadBackendKind kind) const
{
    auto it = std::find_if(m_sources.begin(), m_sources.end(),
        [kind](const auto& s) { return s && s->kind() == kind; });
    return it == m_sources.end() ? nullptr : it->get();
}

BackendRegistry::SelectionResult BackendRegistry::select(
    const domain::Stream& stream,
    std::optional<domain::DownloadBackendKind> override) const
{
    policy::BackendSelectionInputs inputs;
    inputs.stream = stream;
    inputs.activeDebrid = m_activeDebrid;
    inputs.override = override;
    inputs.backends.reserve(m_sources.size());
    for (const auto& src : m_sources) {
        if (!src) {
            continue;
        }
        policy::BackendAvailability a;
        a.kind = src->kind();
        a.canHandle = src->canHandle(stream);
        inputs.backends.push_back(a);
    }

    const auto outcome = policy::selectBackend(inputs);
    if (const auto* err
        = std::get_if<policy::BackendSelectionError>(&outcome)) {
        return *err;
    }

    const auto kind = std::get<domain::DownloadBackendKind>(outcome);
    auto* source = find(kind);
    if (!source) {
        // Should not happen — the policy only returns kinds that
        // were in the input list — but guard anyway.
        policy::BackendSelectionError err;
        err.kind = policy::BackendSelectionError::Kind::NoBackend;
        return err;
    }
    return source;
}

} // namespace kinema::playback::transfer
