// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/SimilarStrip.h"

#include "api/TmdbClient.h"
#include "core/HttpErrorPresenter.h"
#include "kinema_debug.h"
#include "ui/DiscoverCardDelegate.h"
#include "ui/DiscoverRowModel.h"
#include "ui/DiscoverSection.h"
#include "ui/ImageLoader.h"
#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QModelIndex>
#include <QVBoxLayout>

namespace kinema::ui {

SimilarStrip::SimilarStrip(
    api::TmdbClient* tmdb, ImageLoader* images, QWidget* parent)
    : QWidget(parent)
    , m_tmdb(tmdb)
    , m_images(images)
{
    m_section = new DiscoverSection(
        i18nc("@label", "More like this"), m_images, this);

    connect(m_section, &DiscoverSection::itemActivated,
        this, &SimilarStrip::onActivated);

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(12, 4, 12, 8);
    root->setSpacing(4);
    root->addWidget(m_section);

    // Default to hidden — shown only once a non-empty recommendation
    // list comes back. Keeps the pane from growing a dead section
    // when TMDB is unconfigured or has nothing to offer.
    hide();
}

void SimilarStrip::onActivated(const QModelIndex& idx)
{
    if (!idx.isValid()) {
        return;
    }
    if (const auto* item = m_section->model()->at(idx.row())) {
        Q_EMIT itemActivated(*item);
    }
}

void SimilarStrip::setContextImdb(api::MediaKind kind, QString imdbId)
{
    if (imdbId.isEmpty() || !m_tmdb || !m_tmdb->hasToken()) {
        hide();
        m_section->model()->reset({});
        m_lastContextKey.clear();
        return;
    }
    const auto key = QStringLiteral("%1:%2")
        .arg(kind == api::MediaKind::Movie
                ? QLatin1Char('m') : QLatin1Char('s'))
        .arg(imdbId);
    if (key == m_lastContextKey && !m_section->model()->rows().isEmpty()) {
        // Same item, already populated — nothing to do.
        return;
    }
    m_lastContextKey = key;

    auto t = loadFor(kind, std::move(imdbId));
    Q_UNUSED(t);
}

QCoro::Task<void> SimilarStrip::loadFor(api::MediaKind kind, QString imdbId)
{
    const auto myEpoch = ++m_epoch;

    m_section->delegate()->resetRequestTracking();
    m_section->showLoading();
    show();

    // Step 1: resolve IMDB id → TMDB id via /find.
    int tmdbId = 0;
    try {
        const auto [id, found] = co_await m_tmdb->findByImdb(imdbId, kind);
        if (myEpoch != m_epoch) {
            co_return;
        }
        tmdbId = id;
        if (tmdbId == 0) {
            // Nothing to recommend on — hide the whole strip silently.
            hide();
            m_section->model()->reset({});
            co_return;
        }
        // /find may return the item under the opposite array; respect
        // what it actually said.
        kind = found;
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        core::describeError(e, "similar/find");
        hide();
        m_section->model()->reset({});
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
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        core::describeError(e, "similar/recommendations");
        hide();
        m_section->model()->reset({});
        co_return;
    }

    if (items.isEmpty()) {
        hide();
        m_section->model()->reset({});
        co_return;
    }

    m_section->model()->reset(std::move(items));
    m_section->showItems();
    show();
}

} // namespace kinema::ui
