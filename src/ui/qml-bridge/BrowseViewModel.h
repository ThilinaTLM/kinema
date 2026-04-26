// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"
#include "core/DateWindow.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <QList>
#include <QObject>
#include <QString>
#include <QVariantList>

#include <QCoro/QCoroTask>

namespace kinema::api {
class TmdbClient;
}

namespace kinema::config {
class BrowseSettings;
}

namespace kinema::ui::qml {

/**
 * Browse page view-model. Wraps `api::TmdbClient::discover()` and
 * `config::BrowseSettings` so QML never touches either directly.
 *
 * State surface mirrors the legacy `ui::BrowsePage` widget:
 *
 *   * Filters are persisted via `BrowseSettings` on every setter.
 *   * The active genre list is fetched lazily per kind through
 *     `TmdbClient::genreList()`; selections that no longer exist
 *     after a kind switch are pruned.
 *   * Pagination uses an explicit "Load more" trigger from the QML
 *     page; `loadMore()` is a no-op when no further pages exist.
 *   * Auth failures (401/403) latch a page-level `authFailed` flag
 *     so the page swaps to a placeholder instead of painting the
 *     same error six times across rails.
 *
 * Results live in the same `DiscoverSectionModel` that Discover
 * uses — `BrowseViewModel` only needs one rail's worth of data, and
 * appending pages is already supported by `appendItems()`.
 *
 * `applyPreset(MediaKind, DiscoverSort)` is the single entry point
 * for "Show all" navigation from a Discover rail. It rewrites
 * `kind`/`sort` (persisted), clears genre selections to avoid
 * cross-kind nonsense, and refreshes — same shape as a manual
 * filter-bar change.
 */
class BrowseViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int kind READ kind WRITE setKind NOTIFY filtersChanged)
    Q_PROPERTY(QList<int> genreIds READ genreIds WRITE setGenreIds NOTIFY filtersChanged)
    Q_PROPERTY(int dateWindow READ dateWindow WRITE setDateWindow NOTIFY filtersChanged)
    Q_PROPERTY(int minRatingPct READ minRatingPct WRITE setMinRatingPct NOTIFY filtersChanged)
    Q_PROPERTY(int sort READ sort WRITE setSort NOTIFY filtersChanged)
    Q_PROPERTY(bool hideObscure READ hideObscure WRITE setHideObscure NOTIFY filtersChanged)
    Q_PROPERTY(QVariantList availableGenres READ availableGenresList NOTIFY availableGenresChanged)
    Q_PROPERTY(QVariantList activeChips READ activeChipsList NOTIFY filtersChanged)
    Q_PROPERTY(DiscoverSectionModel* results READ results CONSTANT)
    Q_PROPERTY(bool tmdbConfigured READ tmdbConfigured NOTIFY tmdbConfiguredChanged)
    Q_PROPERTY(bool authFailed READ authFailed NOTIFY authFailedChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY loadingChanged)
    Q_PROPERTY(bool canLoadMore READ canLoadMore NOTIFY canLoadMoreChanged)

public:
    BrowseViewModel(api::TmdbClient* tmdb,
        config::BrowseSettings& settings,
        QObject* parent = nullptr);

    // ---- filter accessors -----------------------------------------
    int kind() const noexcept { return static_cast<int>(m_kind); }
    void setKind(int kind);

    QList<int> genreIds() const noexcept { return m_genreIds; }
    void setGenreIds(QList<int> ids);

    int dateWindow() const noexcept { return static_cast<int>(m_dateWindow); }
    void setDateWindow(int window);

    int minRatingPct() const noexcept { return m_minRatingPct; }
    void setMinRatingPct(int pct);

    int sort() const noexcept { return static_cast<int>(m_sort); }
    void setSort(int sort);

    bool hideObscure() const noexcept { return m_hideObscure; }
    void setHideObscure(bool on);

    QVariantList availableGenresList() const;
    QVariantList activeChipsList() const;

    DiscoverSectionModel* results() const noexcept { return m_results; }
    bool tmdbConfigured() const noexcept { return m_tmdbConfigured; }
    bool authFailed() const noexcept { return m_authFailed; }
    bool loading() const noexcept { return m_loading; }
    bool canLoadMore() const noexcept;

public Q_SLOTS:
    /// Fetch page 1 with the current filter set. Called once at
    /// construction and after any filter mutation.
    void refresh();

    /// Append the next page's items. No-op when nothing's loaded
    /// yet, when an in-flight request is already running, or when
    /// the last page has been reached.
    void loadMore();

    /// Reset every filter to its widget-era default and refresh.
    void resetFilters();

    /// Drop a single chip from the `activeChips` list, undoing the
    /// matching filter (e.g. clearing one genre id, resetting the
    /// rating slider). Triggers a refresh.
    void removeChip(int index);

    /// QML hook for clicking a poster; routes to
    /// `openMovieRequested` / `openSeriesRequested` based on the
    /// row's stored kind. Browse always has a TMDB id (no IMDB id).
    void activate(int row);

    /// Called by `MainController` when a Discover rail's "Show all"
    /// is triggered. Sets kind + sort, clears genres + rating to
    /// match a fresh discover view, persists, refreshes.
    void applyPreset(int kind, int sort);

Q_SIGNALS:
    void filtersChanged();
    void availableGenresChanged();
    void tmdbConfiguredChanged();
    void authFailedChanged();
    void loadingChanged();
    void canLoadMoreChanged();

    /// Routed by `MainController` to a passive notification for
    /// phase 04; phase 05 wires these to the real detail page push.
    void openMovieRequested(int tmdbId, const QString& title);
    void openSeriesRequested(int tmdbId, const QString& title);

    /// `MainController` forwards to a passive notification for
    /// load-more error feedback that doesn't deserve a full page
    /// swap.
    void statusMessage(const QString& text, int durationMs);

private:
    void restoreFromSettings();
    void persistAll();
    api::DiscoverQuery buildQuery(int page) const;

    QCoro::Task<void> loadPage(int page, bool append);
    QCoro::Task<void> ensureGenresFor(api::MediaKind kind);

    void setLoading(bool on);
    void setTmdbConfigured(bool on);
    void setAuthFailed(bool on);

    api::TmdbClient* m_tmdb;
    config::BrowseSettings& m_settings;
    DiscoverSectionModel* m_results;

    // Mirror of BrowseSettings; setters write through to KConfig.
    api::MediaKind m_kind = api::MediaKind::Movie;
    QList<int> m_genreIds;
    core::DateWindow m_dateWindow = core::DateWindow::ThisYear;
    int m_minRatingPct = 0;
    api::DiscoverSort m_sort = api::DiscoverSort::Popularity;
    bool m_hideObscure = true;

    QList<api::TmdbGenre> m_availableGenres;
    bool m_genresLoadedMovie = false;
    bool m_genresLoadedSeries = false;
    bool m_genresLoadingMovie = false;
    bool m_genresLoadingSeries = false;

    int m_nextPage = 1;
    int m_totalPages = 0;
    bool m_loading = false;
    quint64 m_epoch = 0;

    bool m_tmdbConfigured = false;
    bool m_authFailed = false;
};

} // namespace kinema::ui::qml
