// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QWidget>

class QSplitter;

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
class StreamsPanel;
class TorrentsModel;

/**
 * Full-width movie detail view. Thin compositor over the shared detail
 * building blocks:
 *
 *   - left:  widgets::MetaHeaderWidget
 *   - right: widgets::StreamsPanel
 *
 * The public API stays stable so controllers and MainWindow keep the
 * same calls while the duplicated implementation lives in the shared
 * widgets.
 */
class DetailPane : public QWidget
{
    Q_OBJECT
public:
    /// `tmdb` is optional: when null, the "More like this" strip is
    /// omitted entirely.
    DetailPane(ImageLoader* loader,
        api::TmdbClient* tmdb,
        config::TorrentioSettings& torrentio,
        config::FilterSettings& filter,
        services::StreamActions& actions,
        config::AppearanceSettings& appearance,
        QWidget* parent = nullptr);

    void showIdle();
    void showMetaLoading();
    void showMetaError(const QString& message);
    void setMeta(const api::MetaDetail& meta);

    /// Feed the current item's IMDB id into the "More like this"
    /// strip. Safe to call with empty id (hides the strip). No-op
    /// when the pane was constructed without a TmdbClient.
    void setSimilarContext(api::MediaKind kind, const QString& imdbId);

    void showTorrentsLoading();
    void showTorrentsError(const QString& message);
    void setTorrents(QList<api::Stream> streams);
    void showTorrentsEmpty();
    /// Replace the torrents panel with a "not released yet" state.
    /// MainWindow calls this instead of kicking off Torrentio when the
    /// item's release date is in the future; the message surfaces the
    /// actual release date so the user knows when to come back.
    void showTorrentsUnreleased(const QDate& releaseDate);

    /// Show or hide the Real-Debrid UI bits (cached-only checkbox,
    /// decoration icons on the torrents table).
    void setRealDebridConfigured(bool on);

    /// Forwarded to the embedded StreamsPanel so play actions can
    /// build a full PlaybackContext.
    void setPlaybackContext(const api::PlaybackContext& ctx);
    /// Live read of the currently-set playback context. Used by
    /// MainWindow to feed the subtitles dialog.
    const api::PlaybackContext& playbackContext() const;

    /// Underlying streams panel — used by MainWindow to install the
    /// shared "Subtitles…" action.
    StreamsPanel* streamsPanel() const { return m_streams; }

    TorrentsModel* torrentsModel() const;

Q_SIGNALS:
    /// Emitted when the user activates a card in the "More like this"
    /// strip. MainWindow resolves the TMDB id and opens the existing
    /// detail flow.
    void similarActivated(const api::DiscoverItem& item);

private:
    QSplitter* m_split {};
    MetaHeaderWidget* m_header {};
    StreamsPanel* m_streams {};
    config::AppearanceSettings& m_appearanceSettings;
};

} // namespace kinema::ui
