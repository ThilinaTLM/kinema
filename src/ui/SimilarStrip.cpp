// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/SimilarStrip.h"

#include "api/TmdbClient.h"
#include "core/HttpError.h"
#include "kinema_debug.h"
#include "ui/DiscoverCardDelegate.h"
#include "ui/DiscoverRowModel.h"
#include "ui/ImageLoader.h"
#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QFrame>
#include <QLabel>
#include <QListView>
#include <QStackedWidget>
#include <QVBoxLayout>

namespace kinema::ui {

SimilarStrip::SimilarStrip(
    api::TmdbClient* tmdb, ImageLoader* images, QWidget* parent)
    : QWidget(parent)
    , m_tmdb(tmdb)
    , m_images(images)
{
    m_title = new QLabel(i18nc("@label", "More like this"), this);
    auto f = m_title->font();
    f.setBold(true);
    f.setPointSizeF(f.pointSizeF() * 1.05);
    m_title->setFont(f);

    m_model = new DiscoverRowModel(this);

    m_view = new QListView(this);
    m_view->setModel(m_model);
    m_view->setFlow(QListView::LeftToRight);
    m_view->setWrapping(false);
    m_view->setMovement(QListView::Static);
    m_view->setUniformItemSizes(true);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setHorizontalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_view->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_view->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_view->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_view->setMouseTracking(true);
    m_view->setFrameShape(QFrame::NoFrame);
    m_view->setResizeMode(QListView::Adjust);
    m_view->setSpacing(4);
    m_view->setFixedHeight(DiscoverCardDelegate::kCardHeight + 12);

    m_delegate = new DiscoverCardDelegate(m_images, m_view);
    m_view->setItemDelegate(m_delegate);

    auto emitActivated = [this](const QModelIndex& idx) {
        if (!idx.isValid()) {
            return;
        }
        if (const auto* item = m_model->at(idx.row())) {
            Q_EMIT itemActivated(*item);
        }
    };
    connect(m_view, &QListView::activated, this, emitActivated);
    connect(m_view, &QListView::clicked, this, emitActivated);

    m_state = new StateWidget(this);
    m_state->setFixedHeight(DiscoverCardDelegate::kCardHeight + 12);

    m_stack = new QStackedWidget(this);
    m_stack->addWidget(m_state);
    m_stack->addWidget(m_view);
    m_stack->setFixedHeight(DiscoverCardDelegate::kCardHeight + 12);

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(12, 4, 12, 8);
    root->setSpacing(4);
    root->addWidget(m_title);
    root->addWidget(m_stack);

    // Default to hidden \u2014 shown only once a non-empty recommendation
    // list comes back. Keeps the pane from growing a dead section when
    // TMDB is unconfigured or has nothing to offer.
    hide();
}

void SimilarStrip::setContextImdb(api::MediaKind kind, QString imdbId)
{
    if (imdbId.isEmpty() || !m_tmdb || !m_tmdb->hasToken()) {
        hide();
        m_model->reset({});
        m_lastContextKey.clear();
        return;
    }
    const auto key = QStringLiteral("%1:%2")
        .arg(kind == api::MediaKind::Movie
                ? QLatin1Char('m') : QLatin1Char('s'))
        .arg(imdbId);
    if (key == m_lastContextKey && !m_model->rows().isEmpty()) {
        // Same item, already populated \u2014 nothing to do.
        return;
    }
    m_lastContextKey = key;

    auto t = loadFor(kind, std::move(imdbId));
    Q_UNUSED(t);
}

QCoro::Task<void> SimilarStrip::loadFor(api::MediaKind kind, QString imdbId)
{
    const auto myEpoch = ++m_epoch;

    m_delegate->resetRequestTracking();
    m_state->showLoading(QString {});
    m_stack->setCurrentWidget(m_state);
    show();

    // Step 1: resolve IMDB id \u2192 TMDB id via /find.
    int tmdbId = 0;
    try {
        const auto [id, found] = co_await m_tmdb->findByImdb(imdbId, kind);
        if (myEpoch != m_epoch) {
            co_return;
        }
        tmdbId = id;
        if (tmdbId == 0) {
            // Nothing to recommend on \u2014 hide the whole strip silently.
            hide();
            m_model->reset({});
            co_return;
        }
        // /find may return the item under the opposite array; respect
        // what it actually said.
        kind = found;
    } catch (const core::HttpError& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        qCDebug(KINEMA) << "Similar: /find failed:" << e.httpStatus()
                        << e.message();
        hide();
        m_model->reset({});
        co_return;
    } catch (const std::exception&) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        hide();
        m_model->reset({});
        co_return;
    }

    // Step 2: recommendations, with a /similar fallback.
    QList<api::DiscoverItem> items;
    try {
        items = co_await m_tmdb->recommendations(kind, tmdbId);
        if (myEpoch != m_epoch) {
            co_return;
        }
        if (items.isEmpty()) {
            items = co_await m_tmdb->similar(kind, tmdbId);
            if (myEpoch != m_epoch) {
                co_return;
            }
        }
    } catch (const core::HttpError& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        qCDebug(KINEMA) << "Similar: recommendations failed:"
                        << e.httpStatus() << e.message();
        hide();
        m_model->reset({});
        co_return;
    } catch (const std::exception&) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        hide();
        m_model->reset({});
        co_return;
    }

    if (items.isEmpty()) {
        hide();
        m_model->reset({});
        co_return;
    }

    m_model->reset(std::move(items));
    m_stack->setCurrentWidget(m_view);
    show();
}

} // namespace kinema::ui
