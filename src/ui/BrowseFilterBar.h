// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "core/DateWindow.h"

#include <QList>
#include <QWidget>

class QActionGroup;
class QCheckBox;
class QMenu;
class QToolButton;

namespace kinema::config {
class BrowseSettings;
}

namespace kinema::ui {

/**
 * Horizontal filter strip that drives the Browse page. Controls from
 * left to right:
 *
 *   [ Movies | TV ]  Genres \u25be  Window \u25be  Rating \u25be  Sort \u25be  [x] Hide obscure
 *
 * The bar is a pure view over `Config::browse*`: every control change
 * is write-through (setters on Config) *and* emits `filtersChanged()`
 * so the BrowsePage can debounce a refetch.
 *
 * Genre list is populated externally via `setAvailableGenres(QList)`
 * \u2014 the page fetches it once per kind through TmdbClient and feeds
 * it back in. When the list changes, any previously-selected ids that
 * no longer exist are dropped from the persisted state.
 */
class BrowseFilterBar : public QWidget
{
    Q_OBJECT
public:
    BrowseFilterBar(config::BrowseSettings& settings,
        QWidget* parent = nullptr);

    /// Build an api::DiscoverQuery from the current control state.
    /// `page` is set to 1 \u2014 BrowsePage bumps it for Load-more.
    api::DiscoverQuery currentQuery() const;

    /// Replace the genre menu with the list appropriate for the
    /// currently-selected kind. Triggers dropping any checked ids
    /// that aren't present, which may emit `filtersChanged()`.
    void setAvailableGenres(QList<api::TmdbGenre> genres);

    /// Clear all filters back to defaults (kind=Movie, window=ThisYear,
    /// sort=Popularity, no genres, rating=Any, hide-obscure=on) and
    /// persist. Emits `filtersChanged()` once.
    void resetToDefaults();

Q_SIGNALS:
    /// Fired after any control changes its value. The BrowsePage
    /// debounces a refetch on this.
    void filtersChanged();

private:
    // Builders for the four popup menus.
    void buildKindToggle();
    void buildWindowMenu();
    void buildRatingMenu();
    void buildSortMenu();
    QMenu* buildGenreMenu();   // fresh menu attached on each
                               // setAvailableGenres() invocation.

    // Sync UI labels with underlying state after reads / writes.
    void applyKindLabel();
    void applyWindowLabel();
    void applyRatingLabel();
    void applySortLabel();
    void applyGenreLabel();

    // State mutators \u2014 write through to Config, update UI, emit.
    void applyKind(api::MediaKind k);
    void applyWindow(core::DateWindow w);
    void applyRating(int ratingPct);
    void applySort(api::DiscoverSort s);
    void applyGenre(int genreId, bool checked);
    void applyHideObscure(bool on);

    // Load initial state from BrowseSettings into m_*.
    void restoreFromConfig();

    config::BrowseSettings& m_settings;

    QToolButton* m_kindBtn {};
    QAction* m_kindMovieAction {};
    QAction* m_kindSeriesAction {};

    QToolButton* m_genresBtn {};
    QMenu* m_genresMenu {};

    QToolButton* m_windowBtn {};
    QActionGroup* m_windowGroup {};

    QToolButton* m_ratingBtn {};
    QActionGroup* m_ratingGroup {};

    QToolButton* m_sortBtn {};
    QActionGroup* m_sortGroup {};

    QCheckBox* m_hideObscure {};

    // Mirror of the Config state, kept in-sync. currentQuery() reads
    // these rather than hitting Config repeatedly.
    api::MediaKind m_kind = api::MediaKind::Movie;
    QList<int> m_genreIds;
    core::DateWindow m_window = core::DateWindow::ThisYear;
    int m_minRatingPct = 0;
    api::DiscoverSort m_sort = api::DiscoverSort::Popularity;
    bool m_hideObscureChecked = true;

    QList<api::TmdbGenre> m_availableGenres;

    // When true, setters suppress filtersChanged() emission. Used
    // while restoring from Config or rebuilding the genre menu.
    bool m_muted = false;
};

} // namespace kinema::ui
