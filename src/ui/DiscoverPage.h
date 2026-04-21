// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Types.h"

#include <QModelIndex>
#include <QVector>
#include <QWidget>

#include <QCoro/QCoroTask>

class QLabel;
class QScrollArea;
class QStackedWidget;
class QVBoxLayout;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::ui {

class DiscoverSection;
class ImageLoader;

/**
 * Home "discover" surface that replaces the idle state in
 * MainWindow's results stack. Presents a vertical stack of
 * sections (trending, popular, top rated, \u2026) fed by TMDB catalog
 * endpoints.
 *
 * Each section is a DiscoverSection widget \u2014 a titled wrap-grid of
 * cards with a "Show more / Show less" toggle. Only this page's
 * outer QScrollArea scrolls, vertically.
 *
 * Per-section concurrency:
 *   - Independent DiscoverRowModel / DiscoverGridView / StateWidget
 *     (owned by DiscoverSection).
 *   - Independent epoch so a stale refresh can't overwrite newer data
 *     if the user hits Settings \u2192 Apply multiple times in a row.
 *
 * Page-level states:
 *   - Not configured: a single centered call-to-action replacing the
 *     rows, with an "Open Settings" button. Shown when there is no
 *     effective TMDB token (user empty AND compile-time default empty).
 *   - Auth failure: same presentation, different copy, entered the
 *     first time any row sees a 401 so we don't spam per-row errors.
 *
 * Click emits itemActivated(DiscoverItem) \u2014 the owning MainWindow
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

    /// Re-fetch all sections. Safe to call repeatedly; in-flight
    /// sections are superseded by epoch and discarded on completion.
    void refresh();

    /// Show the "configure TMDB to enable Discover" call-to-action.
    /// Called when the effective token is empty.
    void showNotConfigured();

Q_SIGNALS:
    /// Emitted when the user activates (Enter / double-click / single
    /// click) a card in any section. MainWindow resolves the TMDB id
    /// and opens the existing movie/series detail flow.
    void itemActivated(const api::DiscoverItem& item);

    /// Emitted by the "Open Settings" button on the not-configured /
    /// auth-failed state. MainWindow shows the SettingsDialog.
    void settingsRequested();

private:
    /// Descriptor for a single section. Populated in buildSections().
    struct Section {
        DiscoverSection* widget {};
        quint64 epoch = 0;
        /// Kicks off a fetch on the TmdbClient for this section.
        /// Assigned in buildSections() as a lambda that captures the
        /// endpoint callable.
        std::function<QCoro::Task<QList<api::DiscoverItem>>()> fetch;
    };

    void buildSections();
    void showPageError(const QString& title, const QString& body);
    QCoro::Task<void> loadSection(int index);
    void onSectionActivated(int sectionIndex, const QModelIndex& idx);

    api::TmdbClient* m_tmdb;
    ImageLoader* m_images;

    QStackedWidget* m_pageStack {}; // { rowsScroll, pageCta }
    QWidget* m_pageCta {};
    QLabel* m_pageCtaTitle {};
    QLabel* m_pageCtaBody {};
    QScrollArea* m_rowsScroll {};
    QWidget* m_rowsContainer {};
    QVBoxLayout* m_rowsLayout {};

    QVector<Section> m_sections;

    /// Latched once any section sees 401 during a refresh so
    /// subsequent 401s on other sections don't keep rewriting the
    /// page state. Cleared at the start of each refresh().
    bool m_pageAuthFailed = false;
};

} // namespace kinema::ui
