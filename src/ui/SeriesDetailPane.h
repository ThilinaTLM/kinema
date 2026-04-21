// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QWidget>

class QCheckBox;
class QLabel;
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
 * Right-side series detail pane. Replaces the old full-window
 * SeriesFocusView: instead of taking over the whole window, it sits in
 * MainWindow's right splitter slot next to the results grid, and can be
 * closed back to a grid-only layout.
 *
 *   +------------------------------------------------------------+
 *   | [Poster] Title (Year)                                  [×] |
 *   |          Genre, Genre · 50 min · ★ 7.8                    |
 *   |          One-paragraph description …                      |
 *   +------------------------------------------------------------+
 *   | Season: [ 1 ▾ ]                [✓ Cached on RD only]      |
 *   +----------------------------+-------------------------------+
 *   | Episodes list              | Torrents table                |
 *   |  S1E1  Pilot               | Release    Q   Size  Sdrs  RD|
 *   |  S1E2  The Way Things Are  | …                             |
 *   |  …                         |                               |
 *   +----------------------------+-------------------------------+
 *                 (draggable horizontal splitter)
 *
 * Landscape-optimised: episodes and torrents sit side-by-side instead
 * of stacked vertically. Poster + meta banner and the season/filter
 * row span the pane width above the split.
 *
 * Owns the SeriesPicker, the torrents QTableView + TorrentsModel, and
 * the cached-only checkbox. MainWindow pushes a SeriesDetail into it
 * via setSeries() and handles the episodeSelected / copyMagnet / …
 * signals the same way it handles DetailPane's.
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

    /// Populate the pane. Auto-selects S1E1 and emits episodeSelected
    /// on the first non-special episode so MainWindow can fetch streams.
    void setSeries(const api::SeriesDetail& series);

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

    void setRealDebridConfigured(bool on);

    /// Give keyboard focus to the episode list. Called by MainWindow
    /// just after opening the pane.
    void focusEpisodeList();

Q_SIGNALS:
    /// Emitted when the user clicks the [×] button in the header.
    void closeRequested();
    void episodeSelected(const api::Episode& episode);
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
    void applyDefaultSplitterRatio();

    ImageLoader* m_loader;
    bool m_rdConfigured = false;

    // Header
    QToolButton* m_closeButton {};
    QLabel* m_posterLabel {};
    QLabel* m_titleLabel {};
    QLabel* m_metaLineLabel {};
    QLabel* m_descLabel {};
    QUrl m_pendingPosterUrl;

    // Horizontal body splitter (episodes | torrents)
    QSplitter* m_bodySplit {};

    // Left: season picker + episode list
    QStackedWidget* m_pickerStack {};
    StateWidget* m_pickerState {};
    SeriesPicker* m_picker {};

    // Right: cached-only + torrents
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
