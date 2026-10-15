// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/BackendSelectionPolicy.h"

#include <KLocalizedString>

namespace kinema::playback::policy {

bool isDebridBackend(domain::DownloadBackendKind kind) noexcept
{
    return kind == domain::DownloadBackendKind::RealDebridHttp
        || kind == domain::DownloadBackendKind::AllDebridHttp;
}

bool matchesActiveDebrid(domain::DownloadBackendKind kind,
    domain::DebridProvider provider) noexcept
{
    switch (provider) {
    case domain::DebridProvider::RealDebrid:
        return kind == domain::DownloadBackendKind::RealDebridHttp;
    case domain::DebridProvider::AllDebrid:
        return kind == domain::DownloadBackendKind::AllDebridHttp;
    case domain::DebridProvider::None:
        break;
    }
    return false;
}

QString providerDisplayName(domain::DebridProvider provider)
{
    switch (provider) {
    case domain::DebridProvider::RealDebrid:
        return i18nc("@info debrid provider name", "Real-Debrid");
    case domain::DebridProvider::AllDebrid:
        return i18nc("@info debrid provider name", "AllDebrid");
    case domain::DebridProvider::None:
        break;
    }
    return {};
}

namespace {

const BackendAvailability* find(
    const std::vector<BackendAvailability>& backends,
    domain::DownloadBackendKind kind) noexcept
{
    for (const auto& b : backends) {
        if (b.kind == kind) {
            return &b;
        }
    }
    return nullptr;
}

} // namespace

BackendSelectionResult selectBackend(const BackendSelectionInputs& in)
{
    if (in.override.has_value()) {
        const auto* b = find(in.backends, *in.override);
        if (b && b->canHandle) {
            return *in.override;
        }
        BackendSelectionError err;
        err.kind = BackendSelectionError::Kind::OverrideUnsupported;
        err.message = i18nc("@info:status",
            "The selected download backend cannot serve this stream.");
        return err;
    }

    if (in.activeDebrid != domain::DebridProvider::None) {
        for (const auto& b : in.backends) {
            if (!matchesActiveDebrid(b.kind, in.activeDebrid)) {
                continue;
            }
            if (b.canHandle) {
                return b.kind;
            }
            BackendSelectionError err;
            err.kind = BackendSelectionError::Kind::ActiveProviderUnsupported;
            err.provider = in.activeDebrid;
            err.message = i18nc("@info:status",
                "%1 is your active debrid provider but cannot serve "
                "this stream. Save a token for it in Settings, or "
                "use the row's menu to force libtorrent.",
                providerDisplayName(in.activeDebrid));
            return err;
        }
        // Active debrid backend not registered at all; fall through.
    }

    for (const auto& b : in.backends) {
        if (isDebridBackend(b.kind)) {
            continue;
        }
        if (b.canHandle) {
            return b.kind;
        }
    }

    BackendSelectionError err;
    err.kind = BackendSelectionError::Kind::NoBackend;
    err.message = i18nc("@info:status",
        "No download backend can serve this stream.");
    return err;
}

} // namespace kinema::playback::policy
