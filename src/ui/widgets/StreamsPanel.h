// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QDate>
#include <QWidget>

class QHBoxLayout;

class QAction;
class QCheckBox;
class QStackedWidget;
class QTableView;
class QToolButton;

namespace kinema::config {
class FilterSettings;
class TorrentioSettings;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui {

class StateWidget;
class TorrentsModel;

/**
 * Shared right-column "streams" panel used by DetailPane and
 * SeriesDetailPane. Owns:
 *
 *   - "Cached on Real-Debrid only" checkbox — two-way bound to
 *     TorrentioSettings::cachedOnly() via cachedOnlyChanged + a
 *     QSignalBlocker.
 *   - An unfiltered stream cache (m_rawStreams) + client-side filter
 *     pipeline (cached-only flag + keyword blocklist).
 *   - A QStackedWidget flipping between a StateWidget (idle / loading /
 *     error / empty / unreleased) and a sortable QTableView backed by
 *     TorrentsModel through a SortKeyRole-aware proxy.
 *   - Context menu + double-click routing that calls the injected
 *     StreamActions service directly — no signal bubbling.
 *
 * The panel emits nothing: status messages for the five actions flow
 * out through StreamActions, which MainWindow wires to the status bar
 * once.
 */
class StreamsPanel : public QWidget
{
    Q_OBJECT
public:
    StreamsPanel(config::TorrentioSettings& torrentio,
        config::FilterSettings& filters,
        services::StreamActions& actions,
        QWidget* parent = nullptr);

    void showLoading();
    void showError(const QString& message);
    void showEmpty();
    /// Replace the streams view with a "not released yet" card. Used
    /// when the current movie / episode's release date is in the
    /// future — we don't hit Torrentio for unreleased content.
    void showUnreleased(const QDate& releaseDate);
    void setStreams(QList<api::Stream> streams);

    /// Toggle Real-Debrid-specific UI bits: the cached-only checkbox
    /// is only visible when RD is configured AND there are streams
    /// to filter.
    void setRealDebridConfigured(bool on);

    /// Install the "Subtitles…" action shown on the action row next
    /// to the cached-only checkbox. The panel doesn't own the action
    /// (MainWindow does), so its enabled state stays bound to the
    /// SubtitleController via MainWindow's wiring. Safe to call once;
    /// repeated calls replace the button.
    void setSubtitleAction(QAction* action);

    /// Set the playback context (media identity + display title +
    /// poster) that every stream in this panel belongs to. The
    /// context is merged with each chosen api::Stream before handing
    /// off to StreamActions::play, so the history layer can record
    /// a well-identified entry and the embedded player can resume.
    ///
    /// Panes call this when they swap the current movie or episode.
    void setPlaybackContext(const api::PlaybackContext& ctx);
    const api::PlaybackContext& playbackContext() const noexcept
    {
        return m_context;
    }

    TorrentsModel* model() const { return m_torrents; }

private:
    void applyClientFilters();
    void rebuildCachedOnlyVisibility();
    void onTorrentContextMenu(const QPoint& pos);

    config::TorrentioSettings& m_torrentioSettings;
    config::FilterSettings& m_filterSettings;
    services::StreamActions& m_actions;

    bool m_rdConfigured = false;

    QCheckBox* m_cachedOnlyCheck {};
    QToolButton* m_subtitleButton {};
    QHBoxLayout* m_actionRow {};
    QStackedWidget* m_stack {};
    StateWidget* m_state {};
    QTableView* m_view {};
    TorrentsModel* m_torrents {};

    /// Unfiltered raw results, so the cached-only checkbox can toggle
    /// without a refetch.
    QList<api::Stream> m_rawStreams;

    /// Media identity + display metadata shared by every stream in
    /// this panel. Cleared between panes; updated by the owning
    /// DetailPane / SeriesDetailPane.
    api::PlaybackContext m_context;
};

} // namespace kinema::ui
