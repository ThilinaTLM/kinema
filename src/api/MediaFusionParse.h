// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

namespace kinema::api::mediafusion {

/// Result of parsing a MediaFusion `manifest.json` URL.
struct ManifestUrlParts {
    bool valid = false;
    /// "<scheme>://<host>" (no trailing slash). Empty when `!valid`.
    QString baseUrl;
    /// Opaque encrypted-config token. Empty when the URL points at
    /// an unconfigured instance (`<host>/manifest.json` with no
    /// token segment).
    QString encryptedConfig;
};

/**
 * Parse a MediaFusion manifest URL into its host + encrypted-config
 * components. Accepted shapes:
 *
 *     https://mediafusion.elfhosted.com/manifest.json
 *         \u2192 baseUrl = "https://mediafusion.elfhosted.com",
 *           encryptedConfig = ""
 *
 *     https://mediafusion.elfhosted.com/D-eyJ\u2026XYZ/manifest.json
 *         \u2192 baseUrl = "https://mediafusion.elfhosted.com",
 *           encryptedConfig = "D-eyJ\u2026XYZ"
 *
 * Anything else (no `/manifest.json`, multiple path segments before
 * `manifest.json`, missing scheme/host) returns `{ valid = false }`.
 */
ManifestUrlParts parseManifestUrl(const QString& url);

/// Build the `/stream/{kind}/{streamId}.json` URL for an indexer
/// call against MediaFusion. Combines `parts.baseUrl` with the
/// optional token segment and the stream-id path.
///
/// Returns an empty string when `parts.valid == false`.
QString buildStreamUrl(const ManifestUrlParts& parts,
    const QString& kindStr,
    const QString& streamId);

} // namespace kinema::api::mediafusion
