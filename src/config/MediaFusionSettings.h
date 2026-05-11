// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>
#include <QString>

namespace kinema::config {

/**
 * MediaFusion per-user settings.
 *
 * MediaFusion is configured through its own web UI at
 * `<host>/configure`: users pick streaming indexers, optionally
 * paste their RD / AD / Premiumize keys, choose filters, and get
 * back a personalised manifest URL like
 *
 *     https://mediafusion.elfhosted.com/D-eyJ...XYZ/manifest.json
 *
 * The opaque base64 blob between the host and `/manifest.json` is
 * MediaFusion's "encrypted config" — we treat it as a black box
 * and slot it into the stream URL exactly where MediaFusion
 * expects it.
 *
 * Kinema asks the user to paste that whole manifest URL into one
 * field; on save we extract the host + token and persist both
 * separately so the indexer can build stream URLs without
 * re-parsing each time.
 *
 * KConfig group: [MediaFusion]
 * Keys:
 *   manifestUrl       string  (raw input — round-trips in the UI)
 *   baseUrl           string  (host scheme://host, derived)
 *   encryptedConfig   string  (opaque token, derived; may be empty)
 */
class MediaFusionSettings : public QObject
{
    Q_OBJECT
public:
    explicit MediaFusionSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    /// Raw user-pasted manifest URL. Round-trips in the settings UI.
    QString manifestUrl() const;

    /// Effective `<scheme>://<host>` MediaFusion is reached at.
    QString baseUrl() const;

    /// Encrypted-config token extracted from the manifest URL.
    /// Empty when the user hasn't configured MediaFusion (unconfigured
    /// instances still answer `/stream/...` with a default pool).
    QString encryptedConfig() const;

    /// Save all three derived values in one call from a manifest URL.
    /// On parse failure leaves state unchanged and returns false.
    bool saveFromManifestUrl(const QString& url);

    /// Clear the saved configuration back to defaults.
    void clear();

    /// Default host used when no manifest has been saved.
    static QString defaultBaseUrl();

Q_SIGNALS:
    void manifestUrlChanged(const QString&);
    void baseUrlChanged(const QString&);
    void encryptedConfigChanged(const QString&);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
