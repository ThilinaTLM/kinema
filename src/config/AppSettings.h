// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "config/AppearanceSettings.h"
#include "config/BrowseSettings.h"
#include "config/CacheSettings.h"
#include "config/FilterSettings.h"
#include "config/PlayerSettings.h"
#include "config/RealDebridSettings.h"
#include "config/SearchSettings.h"
#include "config/SubtitleSettings.h"
#include "config/TorrentioSettings.h"
#include "core/TorrentioConfig.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Root of the settings tree. Owns one KSharedConfigPtr and exposes
 * seven concern-grouped sub-settings. Injected at app startup
 * (KinemaApplication::settings()) and passed down by reference.
 *
 * There is no `instance()` accessor — do not reintroduce one. Every
 * collaborator that needs settings should take the relevant
 * sub-settings in its constructor.
 */
class AppSettings : public QObject
{
    Q_OBJECT
public:
    /// Uses KSharedConfig::openConfig().
    explicit AppSettings(QObject* parent = nullptr);

    /// Tests / advanced callers can inject an explicit config.
    explicit AppSettings(KSharedConfigPtr config, QObject* parent = nullptr);

    SearchSettings& search() noexcept { return m_search; }
    const SearchSettings& search() const noexcept { return m_search; }

    PlayerSettings& player() noexcept { return m_player; }
    const PlayerSettings& player() const noexcept { return m_player; }

    FilterSettings& filter() noexcept { return m_filter; }
    const FilterSettings& filter() const noexcept { return m_filter; }

    BrowseSettings& browse() noexcept { return m_browse; }
    const BrowseSettings& browse() const noexcept { return m_browse; }

    TorrentioSettings& torrentio() noexcept { return m_torrentio; }
    const TorrentioSettings& torrentio() const noexcept { return m_torrentio; }

    AppearanceSettings& appearance() noexcept { return m_appearance; }
    const AppearanceSettings& appearance() const noexcept { return m_appearance; }

    RealDebridSettings& realDebrid() noexcept { return m_realDebrid; }
    const RealDebridSettings& realDebrid() const noexcept { return m_realDebrid; }

    SubtitleSettings& subtitle() noexcept { return m_subtitle; }
    const SubtitleSettings& subtitle() const noexcept { return m_subtitle; }

    CacheSettings& cache() noexcept { return m_cache; }
    const CacheSettings& cache() const noexcept { return m_cache; }

    /// Build the Torrentio options from the current Torrentio + Filter
    /// state. The RD token is NOT filled — callers merge it in from
    /// TokenController.
    core::torrentio::ConfigOptions torrentioOptions() const;

Q_SIGNALS:
    /// Fired whenever any input to torrentioOptions() changes
    /// (default sort or filter exclusions). MainWindow/controllers
    /// debounce and refetch the visible torrent list.
    void torrentioOptionsChanged();

private:
    void connectAggregateSignals();

    SearchSettings m_search;
    PlayerSettings m_player;
    FilterSettings m_filter;
    BrowseSettings m_browse;
    TorrentioSettings m_torrentio;
    AppearanceSettings m_appearance;
    RealDebridSettings m_realDebrid;
    SubtitleSettings m_subtitle;
    CacheSettings m_cache;
};

} // namespace kinema::config
