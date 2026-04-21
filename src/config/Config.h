// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"
#include "core/DateWindow.h"
#include "core/Player.h"
#include "core/TorrentioConfig.h"

#include <QByteArray>
#include <QObject>
#include <QString>

namespace kinema::config {

/**
 * Tiny KConfig-backed singleton holding preferences that Kinema wants to
 * survive across restarts. M2 only reads/writes a handful of keys; M4's
 * Settings dialog will grow this.
 *
 * Groups:
 *   [General]
 *     searchKind        = "Movie" | "Series"      (last SearchBar selection)
 *     cachedOnly        = true|false              (RD cached-only checkbox)
 *     defaultSort       = "seeders" | "size" | "qualitysize"
 *                                                  (Torrentio sort= param)
 *     browseSplitter    = <QSplitter::saveState() bytes>
 *                         (grid + detail-panel horizontal splitter)
 *     seriesPaneSplitter= <QSplitter::saveState() bytes>
 *                         (episodes | torrents inside series pane)
 *   [Filters]
 *     excludedResolutions = comma-separated: 4k,1080p,720p,480p,other
 *     excludedCategories  = comma-separated: cam,scr,threed,hdr,hdr10plus,
 *                                            dolbyvision,nonen,unknown,brremux
 *     keywordBlocklist    = newline-separated free-form substrings
 *   [Player]
 *     preferred     = "mpv" | "vlc" | "custom"    (Play action target)
 *     customCommand = <free-form command line>   (only for preferred="custom")
 *   [RealDebrid]
 *     configured    = true|false                 (mirrors "is a token in the keyring?")
 *   [Browse]
 *     kind          = "Movie" | "Series"          (BrowsePage kind toggle)
 *     genreIds      = "28,12,35"                  (CSV of TMDB genre ids)
 *     dateWindow    = "month1"|"month3"|"year"|"year3"|"any"
 *     minRatingPct  = 0 | 60 | 70 | 75 | 80       (rating*10, 0 = Any)
 *     sort          = "popularity"|"releaseDate"|"rating"|"title"
 *     hideObscure   = true|false                  (vote_count.gte=200 toggle)
 *
 * Token material lives in the system keyring, never in this config.
 */
class Config : public QObject
{
    Q_OBJECT
public:
    static Config& instance();

    // Real-Debrid — whether a token is stored in the keyring.
    bool hasRealDebrid() const;
    void setHasRealDebrid(bool);

    // Cached-on-Real-Debrid-only filter.
    bool cachedOnly() const;
    void setCachedOnly(bool);

    // Last-used SearchBar mode.
    api::MediaKind searchKind() const;
    void setSearchKind(api::MediaKind);

    // Default Torrentio sort mode (Seeders / Size / QualitySize).
    core::torrentio::SortMode defaultSort() const;
    void setDefaultSort(core::torrentio::SortMode);

    // Filter defaults.
    QStringList excludedResolutions() const;
    void setExcludedResolutions(QStringList);

    QStringList excludedCategories() const;
    void setExcludedCategories(QStringList);

    QStringList keywordBlocklist() const;
    void setKeywordBlocklist(QStringList);

    // Outer grid / detail-panel horizontal splitter (QSplitter::saveState).
    // Empty on first run — MainWindow falls back to 40/60 default.
    QByteArray browseSplitterState() const;
    void setBrowseSplitterState(QByteArray state);

    // Inner (episodes | torrents) horizontal splitter of the series
    // detail pane (QSplitter::saveState).
    // Empty on first run — SeriesDetailPane falls back to 35/65 default.
    //
    // NOTE: unused since the full-width detail refactor — the inner
    // split no longer exists. Accessors kept for back-compat / future
    // migrations.
    QByteArray seriesPaneSplitterState() const;
    void setSeriesPaneSplitterState(QByteArray state);

    // Detail-view left (description + similar) / right (streams or
    // episodes→streams) horizontal splitter state. Shared by both
    // DetailPane and SeriesDetailPane. Empty on first run — panes fall
    // back to a ~35/65 default.
    QByteArray detailSplitterState() const;
    void setDetailSplitterState(QByteArray state);

    // Preferred external media player for the Play action. Defaults to
    // mpv. The launcher falls back to the other known players if the
    // preferred one isn't on $PATH.
    core::player::Kind preferredPlayer() const;
    void setPreferredPlayer(core::player::Kind);

    // Free-form command line used when preferredPlayer() == Custom.
    // May contain the literal token "{url}" where the URL should be
    // substituted; otherwise the URL is appended as a final argument.
    QString customPlayerCommand() const;
    void setCustomPlayerCommand(const QString& command);

    // ---- Browse page state -----------------------------------------------
    //
    // Persisted filter-bar state for the Browse surface. Every setter
    // is cheap and write-through; BrowsePage writes these directly as
    // the user changes controls so the last-used slice survives
    // restarts. No signal — BrowsePage drives its own refetches.

    api::MediaKind browseKind() const;
    void setBrowseKind(api::MediaKind);

    QList<int> browseGenreIds() const;
    void setBrowseGenreIds(QList<int>);

    core::DateWindow browseDateWindow() const;
    void setBrowseDateWindow(core::DateWindow);

    /// Minimum rating expressed as rating * 10 (so the file holds ints).
    /// 0 means "any"; 60/70/75/80 are the buckets the UI offers.
    int browseMinRatingPct() const;
    void setBrowseMinRatingPct(int);

    api::DiscoverSort browseSort() const;
    void setBrowseSort(api::DiscoverSort);

    bool browseHideObscure() const;
    void setBrowseHideObscure(bool);

    /// Options used to build the Torrentio URL. M2 still uses defaults
    /// (sort = Seeders, no quality/provider filters); the RD token is
    /// merged in by MainWindow from the in-memory cache. M4 will add
    /// filter UI that feeds into this.
    core::torrentio::ConfigOptions defaultTorrentioOptions() const;

Q_SIGNALS:
    void cachedOnlyChanged(bool);
    void realDebridChanged(bool);
    void preferredPlayerChanged(core::player::Kind);
    /// Emitted when any setting that feeds into defaultTorrentioOptions()
    /// changes (defaultSort, excludedResolutions, excludedCategories).
    /// MainWindow listens to this to refetch the visible torrent list.
    /// Sequential setter calls (typical when Settings applies several
    /// pages at once) emit the signal multiple times; consumers should
    /// debounce via QTimer::singleShot(0, …) if they care.
    void torrentioOptionsChanged();
    /// Emitted when keywordBlocklist changes. This is separate because it
    /// only affects the client-side filter, not the server query.
    void keywordBlocklistChanged(QStringList);

private:
    Config();
};

} // namespace kinema::config
