// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "config/AppearanceSettings.h"
#include "config/BrowseSettings.h"
#include "config/CacheSettings.h"
#include "config/DebridSettings.h"
#include "config/DownloadSettings.h"
#include "config/FilterSettings.h"
#include "config/IndexerSettings.h"
#include "config/LibrarySettings.h"
#include "config/PeerflixSettings.h"
#include "config/PlayerSettings.h"
#include "config/SearchSettings.h"
#include "config/SubtitleSettings.h"
#include "config/TorrentioSettings.h"
#include "config/TorrentStreamingSettings.h"

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

    LibrarySettings& library() noexcept { return m_library; }
    const LibrarySettings& library() const noexcept { return m_library; }

    TorrentioSettings& torrentio() noexcept { return m_torrentio; }
    const TorrentioSettings& torrentio() const noexcept { return m_torrentio; }

    AppearanceSettings& appearance() noexcept { return m_appearance; }
    const AppearanceSettings& appearance() const noexcept { return m_appearance; }

    DebridSettings& debrid() noexcept { return m_debrid; }
    const DebridSettings& debrid() const noexcept { return m_debrid; }

    SubtitleSettings& subtitle() noexcept { return m_subtitle; }
    const SubtitleSettings& subtitle() const noexcept { return m_subtitle; }

    CacheSettings& cache() noexcept { return m_cache; }
    const CacheSettings& cache() const noexcept { return m_cache; }

    TorrentStreamingSettings& torrentStreaming() noexcept { return m_torrentStreaming; }
    const TorrentStreamingSettings& torrentStreaming() const noexcept { return m_torrentStreaming; }

    DownloadSettings& download() noexcept { return m_download; }
    const DownloadSettings& download() const noexcept { return m_download; }

    IndexerSettings& indexers() noexcept { return m_indexers; }
    const IndexerSettings& indexers() const noexcept { return m_indexers; }

    PeerflixSettings& peerflix() noexcept { return m_peerflix; }
    const PeerflixSettings& peerflix() const noexcept { return m_peerflix; }

private:
    SearchSettings m_search;
    PlayerSettings m_player;
    FilterSettings m_filter;
    BrowseSettings m_browse;
    LibrarySettings m_library;
    TorrentioSettings m_torrentio;
    AppearanceSettings m_appearance;
    DebridSettings m_debrid;
    SubtitleSettings m_subtitle;
    CacheSettings m_cache;
    TorrentStreamingSettings m_torrentStreaming;
    DownloadSettings m_download;
    IndexerSettings m_indexers;
    PeerflixSettings m_peerflix;
};

} // namespace kinema::config
