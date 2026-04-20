// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QWidget>

class QCheckBox;
class QLabel;
class QPushButton;
class QSplitter;
class QStackedWidget;
class QTableView;

namespace kinema::ui {

class ImageLoader;
class SeriesPicker;
class StateWidget;
class TorrentsModel;

/**
 * Full-window series focus view. Replaces DetailPane's old series mode
 * with a dedicated layout:
 *
 *   +---------------------------------------------------------+
 *   |  ← Back    From (2022)                                  |
 *   |  +----+   Drama, Horror, Mystery · 50 min · ★ 7.8       |
 *   |  |pos |   Unravel the mystery of a town…                |
 *   |  +----+                                                 |
 *   +---------------------------------------------------------+
 *   |  Season: [ 1 ▾ ]                                        |
 *   |  [ episode list — QSplitter top half ]                  |
 *   +---------------------------------------------------------+
 *   |  [✓ Cached on Real-Debrid only]                         |
 *   |  [ torrents table  — QSplitter bottom half ]            |
 *   +---------------------------------------------------------+
 *
 * Owns the SeriesPicker, the torrents QTableView + TorrentsModel, and
 * the cached-only checkbox. MainWindow pushes a SeriesDetail into it
 * via setSeries() and handles the episodeSelected / copyMagnet / …
 * signals the same way it handles DetailPane's.
 */
class SeriesFocusView : public QWidget
{
    Q_OBJECT
public:
    SeriesFocusView(ImageLoader* loader, QWidget* parent = nullptr);

    /// Populate the view. Auto-selects S1E1 and emits episodeSelected
    /// on the first non-special episode so MainWindow can fetch streams.
    void setSeries(const api::SeriesDetail& series);

    void showMetaLoading();
    void showMetaError(const QString& message);

    void showTorrentsLoading();
    void showTorrentsError(const QString& message);
    void setTorrents(QList<api::Stream> streams);
    void showTorrentsEmpty();

    void setRealDebridConfigured(bool on);

    /// Give keyboard focus to the episode list. Called by MainWindow
    /// just after switching into the focus view.
    void focusEpisodeList();

Q_SIGNALS:
    void backRequested();
    void episodeSelected(const api::Episode& episode);
    void copyMagnetRequested(const api::Stream& stream);
    void openMagnetRequested(const api::Stream& stream);
    void copyDirectUrlRequested(const api::Stream& stream);
    void openDirectUrlRequested(const api::Stream& stream);

private:
    void applyCachedOnlyFilter();
    void rebuildCachedOnlyVisibility();
    void onTorrentContextMenu(const QPoint& pos);
    void updatePoster();
    void saveSplitterState();
    void applyDefaultSplitterRatio();

    ImageLoader* m_loader;
    bool m_rdConfigured = false;

    // Header
    QPushButton* m_backButton {};
    QLabel* m_posterLabel {};
    QLabel* m_titleLabel {};
    QLabel* m_metaLineLabel {};
    QLabel* m_descLabel {};
    QUrl m_pendingPosterUrl;

    // Body splitter
    QSplitter* m_bodySplit {};

    // Top: season picker + episode list
    QStackedWidget* m_pickerStack {};
    StateWidget* m_pickerState {};
    SeriesPicker* m_picker {};

    // Bottom: cached-only + torrents
    QCheckBox* m_cachedOnlyCheck {};
    QStackedWidget* m_torrentsStack {};
    StateWidget* m_torrentsState {};
    QTableView* m_torrentsView {};
    TorrentsModel* m_torrents {};

    QList<api::Stream> m_rawStreams;
};

} // namespace kinema::ui
