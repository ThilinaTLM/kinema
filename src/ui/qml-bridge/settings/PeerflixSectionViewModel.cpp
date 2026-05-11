// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/PeerflixSectionViewModel.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "api/IndexerSelector.h"
#include "config/PeerflixSettings.h"
#include "domain/Indexer.h"
#include "domain/Media.h"
#include "kinema_log_ui.h"
#include <KLocalizedString>

namespace kinema::ui::qml::settings {

// ============================== Indexers: Peerflix section ===============

PeerflixSectionViewModel::PeerflixSectionViewModel(
    api::IndexerSelector* indexers,
    config::PeerflixSettings& settings, QObject* parent)
    : QObject(parent)
    , m_indexers(indexers)
    , m_settings(settings)
{
    connect(&m_settings, &config::PeerflixSettings::baseUrlChanged,
        this, &PeerflixSectionViewModel::baseUrlChanged);
}

QString PeerflixSectionViewModel::baseUrl() const
{
    return m_settings.baseUrl();
}

QString PeerflixSectionViewModel::defaultBaseUrlString() const
{
    return config::PeerflixSettings::defaultBaseUrl();
}

void PeerflixSectionViewModel::setBaseUrl(const QString& url)
{
    m_settings.setBaseUrl(url);
}

void PeerflixSectionViewModel::resetBaseUrl()
{
    m_settings.setBaseUrl(config::PeerflixSettings::defaultBaseUrl());
}

void PeerflixSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

void PeerflixSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void PeerflixSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> PeerflixSectionViewModel::testTask()
{
    auto* indexer = m_indexers
        ? m_indexers->find(domain::IndexerKind::Peerflix)
        : nullptr;
    if (!indexer) {
        setStatus(i18nc("@info indexer settings status",
            "Peerflix is not registered."), kStatusError);
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info indexer settings status, in progress",
        "Probing Peerflix\u2026"), kStatusInfo);
    const bool ok = co_await indexer->testConnection();
    if (ok) {
        setStatus(i18nc("@info indexer settings status",
            "Peerflix is reachable."), kStatusPositive);
    } else {
        setStatus(i18nc("@info indexer settings status",
            "Peerflix did not respond. Check the base URL or try again later."),
            kStatusError);
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
