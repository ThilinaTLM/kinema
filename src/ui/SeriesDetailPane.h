// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"

#include <QWidget>

class QLabel;
class QSplitter;
class QStackedWidget;
class QToolButton;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::config {
class AppearanceSettings;
class FilterSettings;
class TorrentioSettings;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui {

class ImageLoader;
class MetaHeaderWidget;
class SeriesPicker;
class StateWidget;
class StreamsPanel;

/**
 * Full-width series detail view. Thin compositor over shared widgets:
 *
 *   - left:  MetaHeaderWidget
 *   - right: stacked episodes page / episode-streams page
 *
 * Public API stays stable for the controllers.
 */
class SeriesDetailPane : public QWidget
{
    Q_OBJECT
public:
    /// `tmdb` is optional: when null, the "More like this" strip is
    /// omitted entirely.
    SeriesDetailPane(ImageLoader* loader,
        api::TmdbClient* tmdb,
        config::TorrentioSettings& torrentio,
        config::FilterSettings& filter,
        services::StreamActions& actions,
        config::AppearanceSettings& appearance,
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

private:
    void saveSplitterState();
    void resetStreamsPage();

    QSplitter* m_split {};
    MetaHeaderWidget* m_header {};

    QStackedWidget* m_rightStack {};

    // Page 0: season picker + episode list.
    QStackedWidget* m_pickerStack {};
    StateWidget* m_pickerState {};
    SeriesPicker* m_picker {};

    // Page 1: back button + breadcrumb + streams panel.
    QToolButton* m_backToEpisodesButton {};
    QLabel* m_episodeBreadcrumbLabel {};
    StreamsPanel* m_streams {};

    config::AppearanceSettings& m_appearanceSettings;
};

} // namespace kinema::ui
