// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/BackendSelector.h"

#include "download/DownloadBackend.h"

#include <KLocalizedString>

#include <stdexcept>

namespace kinema::download {

namespace {

bool isDebridKind(api::DownloadBackendKind k) noexcept
{
    return k == api::DownloadBackendKind::RealDebridHttp
        || k == api::DownloadBackendKind::AllDebridHttp;
}

bool matchesActiveDebrid(api::DownloadBackendKind k,
    api::DebridProvider p) noexcept
{
    switch (p) {
    case api::DebridProvider::RealDebrid:
        return k == api::DownloadBackendKind::RealDebridHttp;
    case api::DebridProvider::AllDebrid:
        return k == api::DownloadBackendKind::AllDebridHttp;
    case api::DebridProvider::None:
        break;
    }
    return false;
}

} // namespace

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
        auto* b = find(*override);
        if (b && b->canHandle(s)) {
            return b;
        }
        // The user explicitly picked a backend; refuse loudly
        // instead of silently falling back to a different one. The
        // caller wraps this into a `Failed` row with a useful
        // message, which is much easier to debug than "why did this
        // download go to torrent when I asked for RD?".
        throw std::runtime_error(i18nc("@info:status",
            "The selected download backend cannot serve this stream.")
                                     .toStdString());
    }
    for (const auto& b : m_backends) {
        if (isDebridKind(b->kind())
            && !matchesActiveDebrid(b->kind(), m_activeDebrid)) {
            // Skip non-active debrid backends during default
            // routing; they remain reachable through the explicit
            // `override` path (per-row override menus, resume of
            // rows persisted against the other provider).
            continue;
        }
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
