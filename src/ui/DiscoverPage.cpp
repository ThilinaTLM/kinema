// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/DiscoverPage.h"

#include "api/TmdbClient.h"
#include "core/HttpError.h"
#include "kinema_debug.h"
#include "ui/DiscoverCardDelegate.h"
#include "ui/DiscoverRowModel.h"
#include "ui/ImageLoader.h"
#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QListView>
#include <QPushButton>
#include <QScrollArea>
#include <QStackedWidget>
#include <QVBoxLayout>

namespace kinema::ui {

namespace {

QListView* makeStripView(QWidget* parent)
{
    auto* v = new QListView(parent);
    v->setFlow(QListView::LeftToRight);
    v->setWrapping(false);
    v->setMovement(QListView::Static);
    v->setUniformItemSizes(true);
    v->setSelectionMode(QAbstractItemView::SingleSelection);
    v->setSelectionBehavior(QAbstractItemView::SelectItems);
    v->setHorizontalScrollMode(QAbstractItemView::ScrollPerPixel);
    v->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
    v->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    v->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    v->setMouseTracking(true);
    v->setFrameShape(QFrame::NoFrame);
    v->setResizeMode(QListView::Adjust);
    v->setSpacing(4);
    return v;
}

} // namespace

DiscoverPage::DiscoverPage(
    api::TmdbClient* tmdb, ImageLoader* images, QWidget* parent)
    : QWidget(parent)
    , m_tmdb(tmdb)
    , m_images(images)
{
    // Page-level call-to-action (shown when TMDB is not configured
    // or the token was rejected). Unlike StateWidget, this widget has
    // an explicit "Open Settings" button that MainWindow wires to
    // SettingsDialog.
    m_pageCta = new QWidget(this);
    {
        auto* icon = new QLabel(m_pageCta);
        icon->setAlignment(Qt::AlignCenter);
        icon->setPixmap(QIcon::fromTheme(QStringLiteral("applications-multimedia"))
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
            this, &DiscoverPage::settingsRequested);

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

    // Rows surface.
    m_rowsContainer = new QWidget;
    m_rowsLayout = new QVBoxLayout(m_rowsContainer);
    m_rowsLayout->setContentsMargins(16, 12, 16, 16);
    m_rowsLayout->setSpacing(12);

    m_rowsScroll = new QScrollArea(this);
    m_rowsScroll->setWidgetResizable(true);
    m_rowsScroll->setFrameShape(QFrame::NoFrame);
    m_rowsScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_rowsScroll->setWidget(m_rowsContainer);

    m_pageStack = new QStackedWidget(this);
    m_pageStack->addWidget(m_rowsScroll); // idx 0 = rows
    m_pageStack->addWidget(m_pageCta);    // idx 1 = call-to-action

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->addWidget(m_pageStack);

    buildRows();

    // Initial state: show rows with per-row loading placeholders. The
    // owning MainWindow is expected to call refresh() or
    // showNotConfigured() once it has resolved the effective token.
    m_pageStack->setCurrentWidget(m_rowsScroll);
    for (auto& row : m_rows) {
        row.state->showLoading(QString {});
        row.stack->setCurrentWidget(row.state);
    }
}

void DiscoverPage::buildRows()
{
    struct RowSpec {
        QString label;
        std::function<QCoro::Task<QList<api::DiscoverItem>>(api::TmdbClient*)> fetch;
    };

    const QVector<RowSpec> specs = {
        { i18nc("@label discover row", "Trending this week"),
            [](api::TmdbClient* c) {
                return c->trending(api::MediaKind::Movie, /*weekly=*/true);
            } },
        { i18nc("@label discover row", "Popular series"),
            [](api::TmdbClient* c) {
                return c->popular(api::MediaKind::Series);
            } },
        { i18nc("@label discover row", "Now playing in theaters"),
            [](api::TmdbClient* c) { return c->nowPlayingMovies(); } },
        { i18nc("@label discover row", "On the air"),
            [](api::TmdbClient* c) { return c->onTheAirSeries(); } },
        { i18nc("@label discover row", "Top rated movies"),
            [](api::TmdbClient* c) {
                return c->topRated(api::MediaKind::Movie);
            } },
        { i18nc("@label discover row", "Top rated series"),
            [](api::TmdbClient* c) {
                return c->topRated(api::MediaKind::Series);
            } },
    };

    m_rows.reserve(specs.size());

    for (int i = 0; i < specs.size(); ++i) {
        const auto& spec = specs.at(i);
        Row r;

        r.title = new QLabel(spec.label, m_rowsContainer);
        auto f = r.title->font();
        f.setBold(true);
        f.setPointSizeF(f.pointSizeF() * 1.1);
        r.title->setFont(f);

        r.model = new DiscoverRowModel(this);
        r.view = makeStripView(m_rowsContainer);
        r.view->setModel(r.model);
        r.view->setFixedHeight(DiscoverCardDelegate::kCardHeight + 12);
        r.delegate = new DiscoverCardDelegate(m_images, r.view);
        r.view->setItemDelegate(r.delegate);

        r.state = new StateWidget(m_rowsContainer);

        r.stack = new QStackedWidget(m_rowsContainer);
        r.stack->addWidget(r.state); // idx 0
        r.stack->addWidget(r.view);  // idx 1
        r.stack->setFixedHeight(DiscoverCardDelegate::kCardHeight + 12);

        // Capture the fetch callable with the client bound in; the row
        // doesn't need to know which endpoint it represents.
        auto* tmdb = m_tmdb;
        const auto fetcher = spec.fetch;
        r.fetch = [tmdb, fetcher]() { return fetcher(tmdb); };

        // Click / Enter → itemActivated. Passing the row index so we can
        // look up the item on the correct model.
        const int rowIndex = i;
        connect(r.view, &QListView::activated, this,
            [this, rowIndex](const QModelIndex& idx) {
                onRowActivated(rowIndex, idx);
            });
        connect(r.view, &QListView::clicked, this,
            [this, rowIndex](const QModelIndex& idx) {
                onRowActivated(rowIndex, idx);
            });

        // Per-row retry reloads just this row.
        connect(r.state, &StateWidget::retryRequested, this,
            [this, rowIndex] {
                auto t = loadRow(rowIndex);
                Q_UNUSED(t);
            });

        m_rowsLayout->addWidget(r.title);
        m_rowsLayout->addWidget(r.stack);

        m_rows.append(std::move(r));
    }

    m_rowsLayout->addStretch(1);
}

void DiscoverPage::onRowActivated(int rowIndex, const QModelIndex& idx)
{
    if (rowIndex < 0 || rowIndex >= m_rows.size() || !idx.isValid()) {
        return;
    }
    const auto* item = m_rows.at(rowIndex).model->at(idx.row());
    if (!item) {
        return;
    }
    Q_EMIT itemActivated(*item);
}

void DiscoverPage::showNotConfigured()
{
    m_pageAuthFailed = false;
    m_pageCtaTitle->setText(
        i18nc("@label", "Set up TMDB to enable Discover"));
    m_pageCtaBody->setText(
        i18nc("@info",
            "Discover uses The Movie Database to surface trending, popular, "
            "and top-rated content. Open Settings → TMDB (Discover) and "
            "paste a v4 Read Access Token to get started."));
    m_pageStack->setCurrentWidget(m_pageCta);
}

void DiscoverPage::showPageError(const QString& title, const QString& body)
{
    m_pageCtaTitle->setText(title);
    m_pageCtaBody->setText(body);
    m_pageStack->setCurrentWidget(m_pageCta);
}

void DiscoverPage::refresh()
{
    m_pageAuthFailed = false;
    m_pageStack->setCurrentWidget(m_rowsScroll);

    for (auto& row : m_rows) {
        row.delegate->resetRequestTracking();
        row.state->showLoading(QString {});
        row.stack->setCurrentWidget(row.state);
    }
    // Fire all rows concurrently.
    for (int i = 0; i < m_rows.size(); ++i) {
        auto t = loadRow(i);
        Q_UNUSED(t);
    }
}

QCoro::Task<void> DiscoverPage::loadRow(int index)
{
    if (index < 0 || index >= m_rows.size()) {
        co_return;
    }
    auto& row = m_rows[index];
    const auto myEpoch = ++row.epoch;

    row.state->showLoading(QString {});
    row.stack->setCurrentWidget(row.state);

    try {
        auto items = co_await row.fetch();
        if (myEpoch != row.epoch) {
            co_return;
        }
        if (items.isEmpty()) {
            row.model->reset({});
            row.state->showIdle(
                i18nc("@info", "Nothing here right now."), QString {});
            row.stack->setCurrentWidget(row.state);
            co_return;
        }
        row.model->reset(std::move(items));
        row.stack->setCurrentWidget(row.view);
    } catch (const core::HttpError& e) {
        if (myEpoch != row.epoch) {
            co_return;
        }
        qCWarning(KINEMA) << "Discover row" << index << "failed:"
                          << e.httpStatus() << e.message();
        if ((e.httpStatus() == 401 || e.httpStatus() == 403)
            && !m_pageAuthFailed) {
            m_pageAuthFailed = true;
            showPageError(
                i18nc("@label", "TMDB rejected the token"),
                i18nc("@info",
                    "The bundled TMDB token may have been revoked, or "
                    "your override is invalid. Open Settings → TMDB "
                    "(Discover) to paste a working token."));
            co_return;
        }
        if (m_pageAuthFailed) {
            // Page-level error already shown; don't clobber.
            co_return;
        }
        row.state->showError(e.message(), /*retryable=*/true);
        row.stack->setCurrentWidget(row.state);
    } catch (const std::exception& e) {
        if (myEpoch != row.epoch) {
            co_return;
        }
        row.state->showError(QString::fromUtf8(e.what()), /*retryable=*/true);
        row.stack->setCurrentWidget(row.state);
    }
}

} // namespace kinema::ui
