// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Types.h"

#include <QWidget>

class QCheckBox;
class QLabel;
class QScrollArea;
class QSplitter;
class QStackedWidget;
class QTableView;
class QToolButton;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::ui {

class ImageLoader;
class SeriesPicker;
class SimilarStrip;
class StateWidget;
class TorrentsModel;

/**
 * Full-width series detail view. Mirrors DetailPane's horizontal
 * two-column layout: left = meta + description + similar strip;
 * right = episodes-→-streams two-page stack.
 *
 *   +-----------------------------------+---------------------------+
 *   | [Poster] Title (Year)         [×] | Season: [ 1 ▾ ]          |
 *   |          Genre · Runtime · ★     | S1E1 Pilot                |
 *   |          Description…             | S1E2 …                   |
 *   |                                   | …                         |
 *   | More like this:                   |                           |
 *   |  [p] [p] [p] …                    |                           |
 *   +-----------------------------------+---------------------------+
 *
 * After clicking an episode the right column flips to:
 *
 *   +-----------------------------------+---------------------------+
 *   | (same left column)                | [← Episodes]  S1E1 · Pilot|
 *   |                                   | [✓ Cached on RD]         |
 *   |                                   | Quality | Release | …    |
 *   +-----------------------------------+---------------------------+
 *
 * Esc (handled by MainWindow) pops stream-→-episode before close.
 */
class SeriesDetailPane : public QWidget
{
    Q_OBJECT
public:
    /// `tmdb` is optional: when null, the "More like this" strip is
    /// omitted entirely.
    SeriesDetailPane(ImageLoader* loader,
        api::TmdbClient* tmdb = nullptr,
        QWidget* parent = nullptr);

    /// Populate the pane. Lands on the episodes page; does not
    /// auto-select any episode. MainWindow fetches streams only
    /// after the user explicitly picks one.
    void setSeries(const api::SeriesDetail& series);

    /// Flip the right column to the streams page for the given
    /// episode and update the breadcrumb. Called by MainWindow (after
    /// the user picks an episode) before kicking off the fetch.
    void showEpisodeStreamsPage(const api::Episode& ep);

    /// Flip the right column back to the episodes page. Used by the
    /// back button and by Esc (via tryPopStreamsPage).
    void showEpisodesPage();

    /// If the right column is currently on the streams page, pop it
    /// back to the episodes page and return true. Otherwise no-op
    /// and return false. Used by MainWindow's Esc shortcut.
    bool tryPopStreamsPage();

    /// Feed the series's IMDB id into the "More like this" strip.
    /// Safe to call with empty id (hides the strip). No-op when the
    /// pane was constructed without a TmdbClient.
    void setSimilarContext(const QString& imdbId);

    void showMetaLoading();
    void showMetaError(const QString& message);

    void showTorrentsLoading();
    void showTorrentsError(const QString& message);
    void setTorrents(QList<api::Stream> streams);
    void showTorrentsEmpty();
    /// Replace the torrents panel with a "not released yet" state,
    /// keyed on the selected episode's air date. MainWindow calls
    /// this instead of dispatching to Torrentio for unaired episodes.
    void showTorrentsUnreleased(const QDate& releaseDate);

    void setRealDebridConfigured(bool on);

    /// Give keyboard focus to the episode list. Called by MainWindow
    /// just after opening the pane.
    void focusEpisodeList();

Q_SIGNALS:
    void episodeSelected(const api::Episode& episode);
    /// Emitted when the user backs out of the streams page (via the
    /// back button or Esc). MainWindow uses this to cancel any
    /// in-flight stream coroutine.
    void backToEpisodesRequested();
    /// Emitted when the user activates a card in the "More like this"
    /// strip. MainWindow resolves the TMDB id and opens the existing
    /// detail flow.
    void similarActivated(const api::DiscoverItem& item);
    void copyMagnetRequested(const api::Stream& stream);
    void openMagnetRequested(const api::Stream& stream);
    void copyDirectUrlRequested(const api::Stream& stream);
    void openDirectUrlRequested(const api::Stream& stream);
    void playRequested(const api::Stream& stream);

private:
    void applyClientFilters();
    void rebuildCachedOnlyVisibility();
    void onTorrentContextMenu(const QPoint& pos);
    void updatePoster();
    void saveSplitterState();

    ImageLoader* m_loader;
    bool m_rdConfigured = false;

    // Left column (meta + description + similar)
    QSplitter* m_split {};
    QStackedWidget* m_leftStack {};  // state | content-scroll
    StateWidget* m_leftState {};
    QScrollArea* m_leftScroll {};
    QLabel* m_posterLabel {};
    QLabel* m_titleLabel {};
    QLabel* m_upcomingBadge {};
    QLabel* m_metaLineLabel {};
    QLabel* m_descLabel {};
    QUrl m_pendingPosterUrl;

    // Right column — two pages (episodes | streams)
    QStackedWidget* m_rightStack {};

    // Page 0: season picker + episode list
    QStackedWidget* m_pickerStack {};
    StateWidget* m_pickerState {};
    SeriesPicker* m_picker {};

    // Page 1: back button + breadcrumb + cached-only + torrents
    QToolButton* m_backToEpisodesButton {};
    QLabel* m_episodeBreadcrumbLabel {};
    QCheckBox* m_cachedOnlyCheck {};
    QStackedWidget* m_torrentsStack {};
    StateWidget* m_torrentsState {};
    QTableView* m_torrentsView {};
    TorrentsModel* m_torrents {};

    QList<api::Stream> m_rawStreams;

    // "More like this" strip. Null when the pane was constructed
    // without a TmdbClient.
    SimilarStrip* m_similar {};
};

} // namespace kinema::ui
