// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QVector>
#include <QWidget>

#include <QCoro/QCoroTask>

class QLabel;
class QListView;
class QScrollArea;
class QStackedWidget;
class QVBoxLayout;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::ui {

class DiscoverCardDelegate;
class DiscoverRowModel;
class ImageLoader;
class StateWidget;

/**
 * Home "discover" surface that replaces the idle state in
 * MainWindow's results stack. Presents a vertical stack of
 * horizontally-scrolling strips (rows) fed by TMDB catalog endpoints.
 *
 * Each row is independent:
 *   - its own DiscoverRowModel (backed by QList<api::DiscoverItem>)
 *   - its own QListView in LeftToRight / non-wrapping flow
 *   - its own StateWidget (loading / error / empty)
 *   - its own epoch counter so a stale refresh can't overwrite newer
 *     data if the user hits Settings → Apply multiple times in a row
 *
 * Page-level states:
 *   - Not configured: a single centered call-to-action replacing the
 *     rows, with an "Open Settings" button. Shown when there is no
 *     effective TMDB token (user empty AND compile-time default empty).
 *   - Auth failure: same presentation, different copy, entered the
 *     first time any row sees a 401 so we don't spam per-row errors.
 *
 * Click emits itemActivated(DiscoverItem) — the owning MainWindow
 * resolves the TMDB id to an IMDB id and feeds the existing detail
 * pipeline.
 */
class DiscoverPage : public QWidget
{
    Q_OBJECT
public:
    DiscoverPage(api::TmdbClient* tmdb,
        ImageLoader* images,
        QWidget* parent = nullptr);

    /// Re-fetch all rows. Safe to call repeatedly; in-flight rows
    /// are superseded by epoch and discarded on completion.
    void refresh();

    /// Show the "configure TMDB to enable Discover" call-to-action.
    /// Called when the effective token is empty.
    void showNotConfigured();

Q_SIGNALS:
    /// Emitted when the user activates (Enter / double-click / single
    /// click) a card in any row. MainWindow resolves the TMDB id and
    /// opens the existing movie/series detail flow.
    void itemActivated(const api::DiscoverItem& item);

    /// Emitted by the "Open Settings" button on the not-configured /
    /// auth-failed state. MainWindow shows the SettingsDialog.
    void settingsRequested();

private:
    /// Descriptor for a single row. Populated in buildRows().
    struct Row {
        QLabel* title {};
        QStackedWidget* stack {}; // {state, view}
        StateWidget* state {};
        QListView* view {};
        DiscoverRowModel* model {};
        DiscoverCardDelegate* delegate {};
        quint64 epoch = 0;
        /// Kicks off a fetch on the TmdbClient for this row's data.
        /// Returns a QCoro::Task<void>; assigned in buildRows() as a
        /// lambda that captures `kind` / endpoint.
        std::function<QCoro::Task<QList<api::DiscoverItem>>()> fetch;
    };

    void buildRows();
    void showPageError(const QString& title, const QString& body);
    QCoro::Task<void> loadRow(int index);
    void onRowActivated(int rowIndex, const QModelIndex& idx);

    api::TmdbClient* m_tmdb;
    ImageLoader* m_images;

    QStackedWidget* m_pageStack {}; // { rowsScroll, pageCta }
    QWidget* m_pageCta {};
    QLabel* m_pageCtaTitle {};
    QLabel* m_pageCtaBody {};
    QScrollArea* m_rowsScroll {};
    QWidget* m_rowsContainer {};
    QVBoxLayout* m_rowsLayout {};

    QVector<Row> m_rows;

    /// Latched once any row sees 401 during a refresh so subsequent
    /// 401s on other rows don't keep rewriting the page state. Cleared
    /// at the start of each refresh().
    bool m_pageAuthFailed = false;
};

} // namespace kinema::ui
