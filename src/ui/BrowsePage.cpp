// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/BrowsePage.h"

#include "api/TmdbClient.h"
#include "core/HttpError.h"
#include "kinema_debug.h"
#include "ui/BrowseFilterBar.h"
#include "ui/DiscoverCardDelegate.h"
#include "ui/DiscoverGridView.h"
#include "ui/DiscoverRowModel.h"
#include "ui/ImageLoader.h"
#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QAbstractItemView>
#include <QFrame>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QPushButton>
#include <QScrollArea>
#include <QStackedWidget>
#include <QTimer>
#include <QVBoxLayout>

namespace kinema::ui {

BrowsePage::BrowsePage(
    api::TmdbClient* tmdb, ImageLoader* images, QWidget* parent)
    : QWidget(parent)
    , m_tmdb(tmdb)
    , m_images(images)
{
    // ---- page-level CTA ---------------------------------------------------
    m_pageCta = new QWidget(this);
    {
        auto* icon = new QLabel(m_pageCta);
        icon->setAlignment(Qt::AlignCenter);
        icon->setPixmap(
            QIcon::fromTheme(QStringLiteral("applications-multimedia"))
                .pixmap(QSize(64, 64)));

        m_pageCtaTitle = new QLabel(m_pageCta);
        m_pageCtaTitle->setAlignment(Qt::AlignCenter);
        auto f = m_pageCtaTitle->font();
        f.setPointSizeF(f.pointSizeF() * 1.2);
        f.setBold(true);
        m_pageCtaTitle->setFont(f);
        m_pageCtaTitle->setWordWrap(true);

        m_pageCtaBody = new QLabel(m_pageCta);
        m_pageCtaBody->setAlignment(Qt::AlignCenter);
        m_pageCtaBody->setWordWrap(true);
        m_pageCtaBody->setTextFormat(Qt::PlainText);

        auto* openSettings = new QPushButton(
            QIcon::fromTheme(QStringLiteral("configure")),
            i18nc("@action:button", "Open Settings"), m_pageCta);
        connect(openSettings, &QPushButton::clicked,
            this, &BrowsePage::settingsRequested);

        auto* btnRow = new QHBoxLayout;
        btnRow->addStretch();
        btnRow->addWidget(openSettings);
        btnRow->addStretch();

        auto* v = new QVBoxLayout(m_pageCta);
        v->addStretch();
        v->addWidget(icon);
        v->addWidget(m_pageCtaTitle);
        v->addWidget(m_pageCtaBody);
        v->addLayout(btnRow);
        v->addStretch();
    }

    // ---- content column --------------------------------------------------
    auto* content = new QWidget(this);

    m_filterBar = new BrowseFilterBar(content);
    connect(m_filterBar, &BrowseFilterBar::filtersChanged, this,
        &BrowsePage::scheduleRefresh);

    // Model + view + delegate, parented to scroll contents below.
    m_model = new DiscoverRowModel(this);
    m_delegate = new DiscoverCardDelegate(m_images, this);

    m_scrollContents = new QWidget;
    auto* inner = new QVBoxLayout(m_scrollContents);
    inner->setContentsMargins(12, 12, 12, 12);
    inner->setSpacing(12);

    // Grid + state share a stack so we can swap between
    // "loading / empty / error" and the live grid without the page
    // layout thrashing.
    m_state = new StateWidget(m_scrollContents);
    connect(m_state, &StateWidget::retryRequested, this,
        &BrowsePage::refresh);

    m_grid = new DiscoverGridView(m_scrollContents);
    m_grid->setMaxVisibleRows(-1); // show all rows
    m_grid->setModel(m_model);
    m_grid->setItemDelegate(m_delegate);
    m_grid->setSelectionMode(QAbstractItemView::SingleSelection);
    connect(m_grid, &QAbstractItemView::activated, this,
        &BrowsePage::onCardActivated);
    connect(m_grid, &QAbstractItemView::clicked, this,
        &BrowsePage::onCardActivated);

    m_gridStack = new QStackedWidget(m_scrollContents);
    m_gridStack->addWidget(m_state); // idx 0 = state overlay
    m_gridStack->addWidget(m_grid);  // idx 1 = grid

    // Load-more footer: a button + a small status label next to it.
    m_loadMoreBar = new QWidget(m_scrollContents);
    m_loadMoreBtn = new QPushButton(
        QIcon::fromTheme(QStringLiteral("view-refresh")),
        i18nc("@action:button browse pagination", "Load more"), m_loadMoreBar);
    m_loadMoreStatus = new QLabel(m_loadMoreBar);
    m_loadMoreStatus->setVisible(false);
    auto* footRow = new QHBoxLayout(m_loadMoreBar);
    footRow->setContentsMargins(0, 4, 0, 4);
    footRow->addStretch();
    footRow->addWidget(m_loadMoreStatus);
    footRow->addWidget(m_loadMoreBtn);
    footRow->addStretch();
    m_loadMoreBar->setVisible(false);
    connect(m_loadMoreBtn, &QPushButton::clicked, this, [this] {
        auto t = loadNextPage();
        Q_UNUSED(t);
    });

    inner->addWidget(m_gridStack, 1);
    inner->addWidget(m_loadMoreBar);
    inner->addStretch(0);

    m_scroll = new QScrollArea(content);
    m_scroll->setWidgetResizable(true);
    m_scroll->setFrameShape(QFrame::NoFrame);
    m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_scroll->setWidget(m_scrollContents);

    auto* col = new QVBoxLayout(content);
    col->setContentsMargins(0, 0, 0, 0);
    col->setSpacing(0);
    col->addWidget(m_filterBar);
    col->addWidget(m_scroll, 1);

    // ---- page stack ------------------------------------------------------
    m_pageStack = new QStackedWidget(this);
    m_pageStack->addWidget(content);  // idx 0 = content
    m_pageStack->addWidget(m_pageCta); // idx 1 = CTA

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->addWidget(m_pageStack);

    // Initial state: content visible, grid shows the loading state.
    m_pageStack->setCurrentIndex(0);
    m_gridStack->setCurrentWidget(m_state);
    m_state->showLoading(i18nc("@info:status", "Loading\u2026"));
}

void BrowsePage::refresh()
{
    if (!m_tmdb || !m_tmdb->hasToken()) {
        showNotConfigured();
        return;
    }
    m_pageAuthFailed = false;
    m_pageStack->setCurrentIndex(0);

    // Make sure the genre menu is populated for the current kind.
    // Fire-and-forget \u2014 missing genres don't block the /discover
    // request.
    const auto q = m_filterBar->currentQuery();
    auto gt = loadGenresFor(q.kind);
    Q_UNUSED(gt);

    auto t = loadFirstPage();
    Q_UNUSED(t);
}

void BrowsePage::showNotConfigured()
{
    m_pageAuthFailed = false;
    showPageCta(
        i18nc("@label", "Set up TMDB to enable Browse"),
        i18nc("@info",
            "Browse uses The Movie Database's /discover endpoint. "
            "Open Settings \u2192 TMDB (Discover) and paste a v4 Read "
            "Access Token to enable filtering."));
}

void BrowsePage::showPageCta(const QString& title, const QString& body)
{
    m_pageCtaTitle->setText(title);
    m_pageCtaBody->setText(body);
    m_pageStack->setCurrentWidget(m_pageCta);
}

void BrowsePage::scheduleRefresh()
{
    if (m_refreshQueued) {
        return;
    }
    m_refreshQueued = true;
    // Zero-delay single-shot: coalesces the burst of filtersChanged()
    // calls that happen when resetToDefaults() flips several setters
    // in a row, or when the user toggles a menu that internally
    // cascades state.
    QTimer::singleShot(0, this, [this] {
        m_refreshQueued = false;
        refresh();
    });
}

void BrowsePage::showGridOrState()
{
    // If there are items in the model, the grid is authoritative;
    // the state widget is only surfaced for empty / loading / error.
    if (m_model && m_model->rowCount() > 0) {
        m_gridStack->setCurrentWidget(m_grid);
    } else {
        m_gridStack->setCurrentWidget(m_state);
    }
    updateLoadMoreVisibility();
}

void BrowsePage::updateLoadMoreVisibility()
{
    if (!m_loadMoreBar) {
        return;
    }
    const bool hasItems = m_model && m_model->rowCount() > 0;
    const bool morePages = m_totalPages > 0
        && (m_page) <= m_totalPages; // m_page is the NEXT page to fetch
    m_loadMoreBar->setVisible(hasItems && morePages);
}

QCoro::Task<void> BrowsePage::loadFirstPage()
{
    if (!m_tmdb) {
        co_return;
    }
    const auto myEpoch = ++m_epoch;
    m_loading = true;
    m_page = 1;
    m_totalPages = 0;

    m_delegate->resetRequestTracking();
    m_model->reset({});
    m_state->showLoading(i18nc("@info:status", "Loading\u2026"));
    m_gridStack->setCurrentWidget(m_state);
    m_loadMoreBar->setVisible(false);
    m_loadMoreStatus->setVisible(false);

    api::DiscoverQuery q = m_filterBar->currentQuery();
    q.page = 1;

    try {
        auto result = co_await m_tmdb->discover(q);
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_totalPages = result.totalPages;
        if (result.items.isEmpty()) {
            m_state->showIdle(
                i18nc("@label browse empty state", "No matches"),
                i18nc("@info",
                    "Nothing found with these filters. Try widening "
                    "the date range or clearing some genres."));
            m_gridStack->setCurrentWidget(m_state);
            m_loadMoreBar->setVisible(false);
            m_loading = false;
            co_return;
        }
        m_model->reset(std::move(result.items));
        m_page = 2; // next page to request
        m_gridStack->setCurrentWidget(m_grid);
        updateLoadMoreVisibility();
        m_loading = false;
    } catch (const core::HttpError& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_loading = false;
        qCWarning(KINEMA) << "Browse /discover failed:"
                          << e.httpStatus() << e.message();
        if ((e.httpStatus() == 401 || e.httpStatus() == 403)
            && !m_pageAuthFailed) {
            m_pageAuthFailed = true;
            showPageCta(
                i18nc("@label", "TMDB rejected the token"),
                i18nc("@info",
                    "The bundled TMDB token may have been revoked, "
                    "or your override is invalid. Open Settings \u2192 "
                    "TMDB (Discover) to paste a working token."));
            co_return;
        }
        if (m_pageAuthFailed) {
            co_return;
        }
        m_state->showError(e.message(), /*retryable=*/true);
        m_gridStack->setCurrentWidget(m_state);
        m_loadMoreBar->setVisible(false);
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_loading = false;
        m_state->showError(QString::fromUtf8(e.what()), /*retryable=*/true);
        m_gridStack->setCurrentWidget(m_state);
        m_loadMoreBar->setVisible(false);
    }
}

QCoro::Task<void> BrowsePage::loadNextPage()
{
    if (!m_tmdb || m_loading || m_page > m_totalPages) {
        co_return;
    }
    const auto myEpoch = m_epoch; // don't bump \u2014 this is additive
    m_loading = true;
    m_loadMoreBtn->setEnabled(false);
    m_loadMoreStatus->setText(
        i18nc("@info:status", "Loading more\u2026"));
    m_loadMoreStatus->setVisible(true);

    api::DiscoverQuery q = m_filterBar->currentQuery();
    q.page = m_page;

    try {
        auto result = co_await m_tmdb->discover(q);
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_totalPages = result.totalPages;
        if (!result.items.isEmpty()) {
            m_model->append(result.items);
        }
        ++m_page;
        m_loadMoreStatus->setVisible(false);
        m_loadMoreBtn->setEnabled(true);
        m_loadMoreBtn->setText(
            i18nc("@action:button browse pagination", "Load more"));
        m_loading = false;
        updateLoadMoreVisibility();
    } catch (const core::HttpError& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_loading = false;
        qCWarning(KINEMA) << "Browse load-more failed:"
                          << e.httpStatus() << e.message();
        m_loadMoreStatus->setText(
            i18nc("@info:status", "Load more failed \u2014 try again."));
        m_loadMoreStatus->setVisible(true);
        m_loadMoreBtn->setEnabled(true);
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_loading = false;
        m_loadMoreStatus->setText(QString::fromUtf8(e.what()));
        m_loadMoreStatus->setVisible(true);
        m_loadMoreBtn->setEnabled(true);
    }
}

QCoro::Task<void> BrowsePage::loadGenresFor(api::MediaKind kind)
{
    // Skip when already loaded / in flight under the current language.
    bool& loaded = kind == api::MediaKind::Movie
        ? m_genresLoadedMovie
        : m_genresLoadedSeries;
    bool& loading = kind == api::MediaKind::Movie
        ? m_genresLoadingMovie
        : m_genresLoadingSeries;
    if (loaded || loading) {
        // Already have genres in the TmdbClient cache; re-feed the
        // filter bar in case this is the first refresh after a kind
        // change (which cleared m_availableGenres).
        if (loaded && m_tmdb && m_filterBar) {
            try {
                auto list = co_await m_tmdb->genreList(kind);
                m_filterBar->setAvailableGenres(std::move(list));
            } catch (...) {
                // Non-fatal; filter bar stays showing "Loading\u2026".
            }
        }
        co_return;
    }
    loading = true;
    try {
        auto list = co_await m_tmdb->genreList(kind);
        loading = false;
        loaded = true;
        if (m_filterBar) {
            m_filterBar->setAvailableGenres(std::move(list));
        }
    } catch (const std::exception& e) {
        loading = false;
        qCWarning(KINEMA) << "Genre list fetch failed:" << e.what();
    }
}

void BrowsePage::onCardActivated(const QModelIndex& idx)
{
    if (!idx.isValid() || !m_model) {
        return;
    }
    const auto* item = m_model->at(idx.row());
    if (!item) {
        return;
    }
    Q_EMIT itemActivated(*item);
}

} // namespace kinema::ui
