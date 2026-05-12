// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Library.h"
#include "domain/PlaybackContext.h"
#include "ui/qml-bridge/LibraryListModel.h"
#include "ui/qml-bridge/LibraryRailModel.h"

#include <QHash>
#include <QList>
#include <QObject>
#include <QString>
#include <QVariantList>

#include <optional>

namespace kinema::config {
class LibrarySettings;
}

namespace kinema::controllers {
class LibraryController;
class WatchedController;
}

namespace kinema::ui::qml {

/// Library page view-model.
///
/// Drives a single Browse-style filterable grid (see `model`) plus
/// three smart rails (`upNextModel`, `airingSoonModel`,
/// `comingUpModel`). Filter axes:
///
///   * `kind`         \u2014 All / Movies / Series
///   * `status`       \u2014 All / Continue / ToWatch / Watched / Upcoming
///   * `genreIds`     \u2014 multi-select against `availableGenres`
///   * `sort`         \u2014 RecentlyAdded / Title / ReleaseDate / Rating
///   * `dateWindow`   \u2014 release-window slice (Browse parity)
///   * `minRatingPct` \u2014 minimum imdb rating in steps of 5
///   * `hideWatched`  \u2014 hide titles whose status is Watched
///
/// `availableGenres` is recomputed on every refresh from the union
/// of every saved title's `genres` (Cinemeta data, populated at save
/// time in v7+ and lazily backfilled for older rows). The genre IDs
/// are local indices into this list and are not stable across
/// rebuilds; the QML side rebinds on `availableGenresChanged`.
class LibraryViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(LibraryListModel* model READ model CONSTANT)
    Q_PROPERTY(LibraryRailModel* upNextModel READ upNextModel CONSTANT)
    Q_PROPERTY(LibraryRailModel* airingSoonModel READ airingSoonModel CONSTANT)
    Q_PROPERTY(LibraryRailModel* recentlyAddedModel READ recentlyAddedModel CONSTANT)

    Q_PROPERTY(KindFilter kind READ kind WRITE setKind NOTIFY filtersChanged)
    Q_PROPERTY(StatusFilter status READ status WRITE setStatus NOTIFY filtersChanged)
    Q_PROPERTY(QList<int> genreIds READ genreIds WRITE setGenreIds NOTIFY filtersChanged)
    Q_PROPERTY(SortMode sort READ sort WRITE setSort NOTIFY filtersChanged)
    Q_PROPERTY(int dateWindow READ dateWindow WRITE setDateWindow NOTIFY filtersChanged)
    Q_PROPERTY(int minRatingPct READ minRatingPct WRITE setMinRatingPct NOTIFY filtersChanged)
    Q_PROPERTY(bool hideWatched READ hideWatched WRITE setHideWatched NOTIFY filtersChanged)

    Q_PROPERTY(bool smartMode READ smartMode WRITE setSmartMode NOTIFY smartModeChanged)

    Q_PROPERTY(QVariantList availableGenres READ availableGenres NOTIFY availableGenresChanged)
    Q_PROPERTY(bool filtersActive READ filtersActive NOTIFY filtersChanged)
    Q_PROPERTY(bool empty READ empty NOTIFY modelChanged)
    Q_PROPERTY(bool libraryEmpty READ libraryEmpty NOTIFY libraryEmptyChanged)
    Q_PROPERTY(int totalCount READ totalCount NOTIFY libraryEmptyChanged)

public:
    enum class KindFilter { All = 0, Movies, Series };
    Q_ENUM(KindFilter)

    enum class StatusFilter {
        All = 0,
        Continue,
        ToWatch,
        Watched,
        Upcoming,
    };
    Q_ENUM(StatusFilter)

    enum class SortMode {
        RecentlyAdded = 0,
        Title,
        ReleaseDate,
        Rating,
    };
    Q_ENUM(SortMode)

    /// Browse uses index-based windows; we mirror it so the dialog
    /// shape can be reused. 4 = "Any time" is the Library default
    /// because a curated collection should display fully by default.
    enum DateWindow {
        DateWindowPastMonth = 0,
        DateWindowPast3Months,
        DateWindowThisYear,
        DateWindowPast3Years,
        DateWindowAny,
    };
    Q_ENUM(DateWindow)

    LibraryViewModel(controllers::LibraryController* library,
        controllers::WatchedController* watched,
        config::LibrarySettings& settings,
        QObject* parent = nullptr);
    ~LibraryViewModel() override;

    LibraryListModel* model() const noexcept { return m_model; }
    LibraryRailModel* upNextModel() const noexcept { return m_upNextModel; }
    LibraryRailModel* airingSoonModel() const noexcept { return m_airingSoonModel; }
    LibraryRailModel* recentlyAddedModel() const noexcept { return m_recentlyAddedModel; }

    KindFilter kind() const noexcept { return m_kind; }
    void setKind(KindFilter v);

    StatusFilter status() const noexcept { return m_status; }
    void setStatus(StatusFilter v);

    QList<int> genreIds() const noexcept { return m_genreIds; }
    void setGenreIds(const QList<int>& v);

    SortMode sort() const noexcept { return m_sort; }
    void setSort(SortMode v);

    int dateWindow() const noexcept { return m_dateWindow; }
    void setDateWindow(int v);

    int minRatingPct() const noexcept { return m_minRatingPct; }
    void setMinRatingPct(int v);

    bool hideWatched() const noexcept { return m_hideWatched; }
    void setHideWatched(bool v);

    /// Persisted Library page mode. `true` shows the Smart rails view
    /// (Up Next / Airing Soon / Recently Added); `false` shows the
    /// filterable List grid. Backed by `config::LibrarySettings`.
    bool smartMode() const noexcept { return m_smartMode; }
    void setSmartMode(bool v);

    QVariantList availableGenres() const noexcept { return m_availableGenres; }
    bool filtersActive() const noexcept;
    bool empty() const noexcept { return m_model->rowCount() == 0; }
    bool libraryEmpty() const noexcept { return m_totalCount == 0; }
    int totalCount() const noexcept { return m_totalCount; }

public Q_SLOTS:
    void refresh();
    void resetFilters();
    void activate(int row);
    void resume(int row);
    void removeFromLibrary(int row);
    void toggleWatched(int row);
    /// Activate a smart-rail card. `railId` is one of `"upNext"`,
    /// `"airingSoon"`, `"recentlyAdded"`; `row` is the index into
    /// that rail's model.
    void activateRail(const QString& railId, int row);

Q_SIGNALS:
    void filtersChanged();
    void availableGenresChanged();
    void modelChanged();
    void libraryEmptyChanged();
    void smartModeChanged();
    void openMovieRequested(const QString& imdbId, const QString& title);
    void openSeriesRequested(const QString& imdbId, const QString& title);
    void openSeriesEpisodeRequested(const QString& imdbId,
        const QString& title, int season, int episode);
    void resumeRequested(const domain::HistoryEntry& entry);
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    enum class ResolvedStatus { Continue, ToWatch, Watched, Upcoming };

    /// One library entry materialized as everything we need to drive
    /// both the grid and the rails. Built once per `refresh()`.
    struct Entry {
        domain::LibraryTitle title;
        ResolvedStatus status = ResolvedStatus::ToWatch;
        // Resume data (movie, or latest series episode resume).
        std::optional<domain::HistoryEntry> resume;
        double resumeProgress = -1.0;
        // Series-only fields.
        QList<domain::LibraryEpisode> episodes;
        const domain::LibraryEpisode* nextToWatch = nullptr;
        const domain::LibraryEpisode* firstUpcoming = nullptr;
        int upcomingCount = 0;
        int totalAired = 0;
        int watchedAired = 0;
    };

    void buildEntries();
    void rebuildAvailableGenres();
    void rebuildRails();
    void rebuildGrid();
    bool entryMatchesFilters(const Entry& e) const;
    LibraryListRow toGridRow(const Entry& e) const;
    static int genreIdForName(const QHash<QString, int>& byName,
        const QString& name);

    controllers::LibraryController* m_library {};
    controllers::WatchedController* m_watched {};
    config::LibrarySettings* m_librarySettings {};

    LibraryListModel* m_model {};
    LibraryRailModel* m_upNextModel {};
    LibraryRailModel* m_airingSoonModel {};
    LibraryRailModel* m_recentlyAddedModel {};

    QList<Entry> m_entries;
    int m_totalCount = 0;

    // Filter state
    KindFilter m_kind = KindFilter::All;
    StatusFilter m_status = StatusFilter::All;
    QList<int> m_genreIds;
    SortMode m_sort = SortMode::RecentlyAdded;
    int m_dateWindow = DateWindowAny;
    int m_minRatingPct = 0;
    bool m_hideWatched = false;

    // Persisted view mode (mirror of LibrarySettings::smartMode).
    bool m_smartMode = true;

    // Genre catalog: list of { id: int, name: QString } maps; the
    // QML reads this through the `availableGenres` property. The
    // companion `m_genreIdByName` lets `entryMatchesFilters` resolve
    // names to ids without recomputing the lookup.
    QVariantList m_availableGenres;
    QHash<QString, int> m_genreIdByName;
};

} // namespace kinema::ui::qml
