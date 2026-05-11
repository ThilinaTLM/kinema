// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/TorrentioSectionViewModel.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "api/IndexerSelector.h"
#include "config/TorrentioSettings.h"
#include "core/util/TorrentioConfig.h"
#include "domain/Indexer.h"
#include "domain/Media.h"
#include "kinema_log_ui.h"
#include <KLocalizedString>

namespace kinema::ui::qml::settings {

namespace {

int sortModeToIndex(core::torrentio::SortMode m)
{
    switch (m) {
    case core::torrentio::SortMode::Seeders: return 0;
    case core::torrentio::SortMode::Size: return 1;
    case core::torrentio::SortMode::QualitySize: return 2;
    }
    return 0;
}

core::torrentio::SortMode indexToSortMode(int idx)
{
    switch (idx) {
    case 1: return core::torrentio::SortMode::Size;
    case 2: return core::torrentio::SortMode::QualitySize;
    default: return core::torrentio::SortMode::Seeders;
    }
}

} // namespace

// ============================== Indexers: Torrentio section ==============

TorrentioSectionViewModel::TorrentioSectionViewModel(
    api::IndexerSelector* indexers,
    config::TorrentioSettings& settings, QObject* parent)
    : QObject(parent)
    , m_indexers(indexers)
    , m_settings(settings)
{
    connect(&m_settings, &config::TorrentioSettings::defaultSortChanged,
        this, &TorrentioSectionViewModel::defaultSortChanged);
    connect(&m_settings, &config::TorrentioSettings::baseUrlChanged,
        this, &TorrentioSectionViewModel::baseUrlChanged);
}

int TorrentioSectionViewModel::defaultSort() const
{
    return sortModeToIndex(m_settings.defaultSort());
}

QString TorrentioSectionViewModel::baseUrl() const
{
    return m_settings.baseUrl();
}

QString TorrentioSectionViewModel::defaultBaseUrlString() const
{
    return config::TorrentioSettings::defaultBaseUrl();
}

void TorrentioSectionViewModel::setDefaultSort(int sort)
{
    const auto next = indexToSortMode(sort);
    if (m_settings.defaultSort() == next) {
        return;
    }
    m_settings.setDefaultSort(next);
    // settings emits defaultSortChanged → we re-emit via connect.
}

void TorrentioSectionViewModel::setBaseUrl(const QString& url)
{
    m_settings.setBaseUrl(url);
}

void TorrentioSectionViewModel::resetBaseUrl()
{
    m_settings.setBaseUrl(config::TorrentioSettings::defaultBaseUrl());
}

void TorrentioSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

void TorrentioSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void TorrentioSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> TorrentioSectionViewModel::testTask()
{
    auto* indexer = m_indexers
        ? m_indexers->find(domain::IndexerKind::Torrentio)
        : nullptr;
    if (!indexer) {
        setStatus(i18nc("@info indexer settings status",
            "Torrentio is not registered."), kStatusError);
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info indexer settings status, in progress",
        "Probing Torrentio\u2026"), kStatusInfo);
    const bool ok = co_await indexer->testConnection();
    if (ok) {
        setStatus(i18nc("@info indexer settings status",
            "Torrentio is reachable."), kStatusPositive);
    } else {
        setStatus(i18nc("@info indexer settings status",
            "Torrentio did not respond. Check the base URL or try again later."),
            kStatusError);
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
