// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QWidget>

class QCheckBox;
class QLabel;
class QScrollArea;
class QSplitter;
class QStackedWidget;
class QTableView;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::ui {

class ImageLoader;
class SimilarStrip;
class StateWidget;
class TorrentsModel;

/**
 * Full-width movie detail view. Two columns separated by a draggable
 * horizontal splitter:
 *
 *   +-----------------------------------------------+------------------+
 *   | [Poster] Title (Year)                         | [✓ Cached on RD] |
 *   |          Genre · Runtime · ★ Rating           +------------------+
 *   |          Description …                        | Quality | Release
 *   |                                               | … torrents table
 *   | More like this:                               |
 *   |  [poster] [poster] [poster] …                 |
 *   +-----------------------------------------------+------------------+
 *
 * The torrents table emits signals via a context menu (copy magnet,
 * open magnet). The owning MainWindow wires those to real actions.
 */
class DetailPane : public QWidget
{
    Q_OBJECT
public:
    /// `tmdb` is optional: when null, the "More like this" strip is
    /// omitted entirely.
    DetailPane(ImageLoader* loader,
        api::TmdbClient* tmdb = nullptr,
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

    TorrentsModel* torrentsModel() const { return m_torrents; }

Q_SIGNALS:
    /// Emitted when the user selects "Copy magnet" from the torrent row menu.
    void copyMagnetRequested(const api::Stream& stream);
    /// Emitted when the user selects "Open magnet" from the torrent row menu.
    void openMagnetRequested(const api::Stream& stream);
    /// Emitted when the user selects "Copy direct URL" for an RD-cached row.
    void copyDirectUrlRequested(const api::Stream& stream);
    /// Emitted when the user selects "Open direct URL" for an RD-cached row.
    void openDirectUrlRequested(const api::Stream& stream);
    /// Emitted when the user selects "Play" on an RD-cached row.
    void playRequested(const api::Stream& stream);
    /// Emitted when the user activates a card in the "More like this"
    /// strip. MainWindow resolves the TMDB id and opens the existing
    /// detail flow.
    void similarActivated(const api::DiscoverItem& item);

private:
    void onTorrentContextMenu(const QPoint& pos);
    void updatePoster();
    void applyClientFilters();
    void rebuildCachedOnlyVisibility();

    ImageLoader* m_loader;
    bool m_rdConfigured = false;

    // Left column (meta + description + similar)
    QSplitter* m_split {};
    QStackedWidget* m_leftStack {};   // state | content-scroll
    StateWidget* m_metaState {};
    QScrollArea* m_leftScroll {};
    QLabel* m_posterLabel {};
    QLabel* m_titleLabel {};
    QLabel* m_upcomingBadge {};
    QLabel* m_metaLineLabel {};
    QLabel* m_descLabel {};
    QUrl m_pendingPosterUrl;

    // Right column (cached-only + torrents)
    QCheckBox* m_cachedOnlyCheck {};
    QStackedWidget* m_torrentsStack {};
    StateWidget* m_torrentsState {};
    QTableView* m_torrentsView {};
    TorrentsModel* m_torrents {};
    /// Unfiltered raw results, so the cached-only checkbox can toggle
    /// without a refetch.
    QList<api::Stream> m_rawStreams;

    // "More like this" strip. Null when the pane was constructed
    // without a TmdbClient (tests, or future kill-switch).
    SimilarStrip* m_similar {};
};

} // namespace kinema::ui
