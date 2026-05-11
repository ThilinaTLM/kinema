// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/MediaFusionSettings.h"

#include "api/MediaFusionParse.h"
#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "MediaFusion";
constexpr auto kKeyManifestUrl = "manifestUrl";
constexpr auto kKeyBaseUrl = "baseUrl";
constexpr auto kKeyEncryptedConfig = "encryptedConfig";
} // namespace

QString MediaFusionSettings::defaultBaseUrl()
{
    return QStringLiteral("https://mediafusion.elfhosted.com");
}

MediaFusionSettings::MediaFusionSettings(KSharedConfigPtr config,
    QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

QString MediaFusionSettings::manifestUrl() const
{
    return detail::read(m_config, kGroup, kKeyManifestUrl, QString {});
}

QString MediaFusionSettings::baseUrl() const
{
    const auto raw = detail::read(m_config, kGroup, kKeyBaseUrl, QString {});
    return raw.isEmpty() ? defaultBaseUrl() : raw;
}

QString MediaFusionSettings::encryptedConfig() const
{
    return detail::read(m_config, kGroup, kKeyEncryptedConfig, QString {});
}

bool MediaFusionSettings::saveFromManifestUrl(const QString& url)
{
    const auto parts = api::mediafusion::parseManifestUrl(url);
    if (!parts.valid) {
        return false;
    }

    const auto prevManifest = manifestUrl();
    const auto prevBase = baseUrl();
    const auto prevToken = encryptedConfig();

    detail::write(m_config, kGroup, kKeyManifestUrl, url.trimmed());
    detail::write(m_config, kGroup, kKeyBaseUrl, parts.baseUrl);
    detail::write(m_config, kGroup, kKeyEncryptedConfig,
        parts.encryptedConfig);

    if (prevManifest != url.trimmed()) {
        Q_EMIT manifestUrlChanged(url.trimmed());
    }
    if (prevBase != parts.baseUrl) {
        Q_EMIT baseUrlChanged(parts.baseUrl);
    }
    if (prevToken != parts.encryptedConfig) {
        Q_EMIT encryptedConfigChanged(parts.encryptedConfig);
    }
    return true;
}

void MediaFusionSettings::clear()
{
    const bool hadManifest = !manifestUrl().isEmpty();
    const bool hadBase = baseUrl() != defaultBaseUrl();
    const bool hadToken = !encryptedConfig().isEmpty();

    detail::write(m_config, kGroup, kKeyManifestUrl, QString {});
    detail::write(m_config, kGroup, kKeyBaseUrl, QString {});
    detail::write(m_config, kGroup, kKeyEncryptedConfig, QString {});

    if (hadManifest) {
        Q_EMIT manifestUrlChanged(QString {});
    }
    if (hadBase) {
        Q_EMIT baseUrlChanged(defaultBaseUrl());
    }
    if (hadToken) {
        Q_EMIT encryptedConfigChanged(QString {});
    }
}

} // namespace kinema::config
