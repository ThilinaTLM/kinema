// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/BackendSelector.h"

#include "download/DownloadBackend.h"

#include <KLocalizedString>

#include <stdexcept>

namespace kinema::download {

namespace {

bool isDebridKind(domain::DownloadBackendKind k) noexcept
{
    return k == domain::DownloadBackendKind::RealDebridHttp
        || k == domain::DownloadBackendKind::AllDebridHttp;
}

bool matchesActiveDebrid(domain::DownloadBackendKind k,
    domain::DebridProvider p) noexcept
{
    switch (p) {
    case domain::DebridProvider::RealDebrid:
        return k == domain::DownloadBackendKind::RealDebridHttp;
    case domain::DebridProvider::AllDebrid:
        return k == domain::DownloadBackendKind::AllDebridHttp;
    case domain::DebridProvider::None:
        break;
    }
    return false;
}

QString activeProviderDisplayName(domain::DebridProvider p)
{
    switch (p) {
    case domain::DebridProvider::RealDebrid:
        return i18nc("@info debrid provider name", "Real-Debrid");
    case domain::DebridProvider::AllDebrid:
        return i18nc("@info debrid provider name", "AllDebrid");
    case domain::DebridProvider::None:
        break;
    }
    return {};
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

DownloadBackend* BackendSelector::select(const domain::Stream& s,
    std::optional<domain::DownloadBackendKind> override) const
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
    // When the user has picked a debrid provider as active, that
    // provider gets first refusal. If it can't serve the stream
    // (most commonly because no token is saved for it yet, e.g. the
    // radio was flipped from AllDebrid -> Real-Debrid without
    // saving an RD token), refuse with a clear message rather than
    // silently dropping back to libtorrent. Torrent is still
    // reachable via the per-row "Play via torrent" override.
    if (m_activeDebrid != domain::DebridProvider::None) {
        for (const auto& b : m_backends) {
            if (!matchesActiveDebrid(b->kind(), m_activeDebrid)) {
                continue;
            }
            if (b->canHandle(s)) {
                return b.get();
            }
            throw std::runtime_error(i18nc("@info:status",
                "%1 is your active debrid provider but cannot serve "
                "this stream. Save a token for it in Settings, or "
                "use the row's menu to force libtorrent.",
                activeProviderDisplayName(m_activeDebrid))
                                         .toStdString());
        }
        // Active debrid backend isn't registered at all (shouldn't
        // happen in production; defensive). Fall through.
    }
    for (const auto& b : m_backends) {
        if (isDebridKind(b->kind())) {
            // Skip every debrid backend in default routing when
            // either no provider is active, or none of them matched
            // above. Non-active debrid backends remain reachable
            // through the explicit `override` path (per-row
            // override menus, resume of rows persisted against the
            // other provider).
            continue;
        }
        if (b->canHandle(s)) {
            return b.get();
        }
    }
    return nullptr;
}

DownloadBackend* BackendSelector::find(domain::DownloadBackendKind kind) const
{
    for (const auto& b : m_backends) {
        if (b->kind() == kind) {
            return b.get();
        }
    }
    return nullptr;
}

} // namespace kinema::download
