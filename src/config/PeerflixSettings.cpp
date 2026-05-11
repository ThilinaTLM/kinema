// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/PeerflixSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "Peerflix";
constexpr auto kKeyBaseUrl = "baseUrl";
} // namespace

QString PeerflixSettings::defaultBaseUrl()
{
    return QStringLiteral("https://peerflix.mov");
}

PeerflixSettings::PeerflixSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

QString PeerflixSettings::baseUrl() const
{
    const auto raw = detail::read(m_config, kGroup, kKeyBaseUrl,
        defaultBaseUrl());
    return raw.isEmpty() ? defaultBaseUrl() : raw;
}

void PeerflixSettings::setBaseUrl(const QString& url)
{
    const auto trimmed = url.trimmed();
    const auto effective = trimmed.isEmpty() ? defaultBaseUrl() : trimmed;
    if (baseUrl() == effective) {
        return;
    }
    detail::write(m_config, kGroup, kKeyBaseUrl, effective);
    Q_EMIT baseUrlChanged(effective);
}

} // namespace kinema::config
