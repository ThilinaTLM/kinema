// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/BackendSelector.h"

#include "download/DownloadBackend.h"

namespace kinema::download {

BackendSelector::BackendSelector() = default;
BackendSelector::~BackendSelector() = default;

void BackendSelector::registerBackend(
    std::unique_ptr<DownloadBackend> backend)
{
    if (backend) {
        m_backends.push_back(std::move(backend));
    }
}

DownloadBackend* BackendSelector::select(const api::Stream& s,
    std::optional<api::DownloadBackendKind> override) const
{
    if (override.has_value()) {
        if (auto* b = find(*override); b && b->canHandle(s)) {
            return b;
        }
        // Override requested but the backend can't serve this
        // stream; fall through to the priority list rather than
        // refusing outright.
    }
    for (const auto& b : m_backends) {
        if (b->canHandle(s)) {
            return b.get();
        }
    }
    return nullptr;
}

DownloadBackend* BackendSelector::find(api::DownloadBackendKind kind) const
{
    for (const auto& b : m_backends) {
        if (b->kind() == kind) {
            return b.get();
        }
    }
    return nullptr;
}

} // namespace kinema::download
