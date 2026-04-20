// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QWidget>

class QCheckBox;
class QLabel;
class QStackedWidget;
class QTableView;

namespace kinema::ui {

class ImageLoader;
class StateWidget;
class TorrentsModel;

/**
 * Right-hand pane: large poster, metadata, then the torrents table.
 *
 * The torrents table emits signals via a context menu (copy magnet, open
 * magnet). The owning MainWindow wires those to real actions.
 */
class DetailPane : public QWidget
{
    Q_OBJECT
public:
    DetailPane(ImageLoader* loader, QWidget* parent = nullptr);

    void showIdle();
    void showMetaLoading();
    void showMetaError(const QString& message);
    void setMeta(const api::MetaDetail& meta);

    void showTorrentsLoading();
    void showTorrentsError(const QString& message);
    void setTorrents(QList<api::Stream> streams);
    void showTorrentsEmpty();

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

private:
    void onTorrentContextMenu(const QPoint& pos);
    void updatePoster();
    void applyCachedOnlyFilter();
    void rebuildCachedOnlyVisibility();

    ImageLoader* m_loader;
    bool m_rdConfigured = false;

    // Meta side
    QStackedWidget* m_metaStack;
    StateWidget* m_metaState;
    QWidget* m_metaContent;
    QLabel* m_posterLabel;
    QLabel* m_titleLabel;
    QLabel* m_metaLineLabel;
    QLabel* m_descLabel;
    QUrl m_pendingPosterUrl;

    // Torrents side
    QCheckBox* m_cachedOnlyCheck;
    QStackedWidget* m_torrentsStack;
    StateWidget* m_torrentsState;
    QTableView* m_torrentsView;
    TorrentsModel* m_torrents;
    /// Unfiltered raw results, so the cached-only checkbox can toggle
    /// without a refetch.
    QList<api::Stream> m_rawStreams;
};

} // namespace kinema::ui
