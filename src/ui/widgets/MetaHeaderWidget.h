// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"

#include <QUrl>
#include <QWidget>

class QLabel;
class QScrollArea;
class QStackedWidget;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::ui {

class ImageLoader;
class SimilarStrip;
class StateWidget;

/**
 * Shared left-column "meta header" used by DetailPane and
 * SeriesDetailPane. Owns:
 *
 *   - A QStackedWidget flipping between a StateWidget (idle / loading /
 *     error) and a scrollable content view.
 *   - Poster, title + "Upcoming" badge row, meta line
 *     (genre · runtime · ★ rating), plain-text description.
 *   - "More like this" SimilarStrip when a TmdbClient was supplied.
 *
 * The pane that embeds this widget calls setMeta() when the Cinemeta
 * fetch completes, or show{Loading,Error,Idle}() to drive the state
 * view. setSimilarContext() forwards to the strip (safe to call with
 * an empty IMDB id; no-op when constructed without a TmdbClient).
 */
class MetaHeaderWidget : public QWidget
{
    Q_OBJECT
public:
    MetaHeaderWidget(ImageLoader* loader,
        api::TmdbClient* tmdb /*nullable*/,
        QWidget* parent = nullptr);

    void showIdle();
    void showLoading();
    void showError(const QString& message);
    void setMeta(const api::MetaDetail& meta);
    void setSimilarContext(api::MediaKind kind, const QString& imdbId);

Q_SIGNALS:
    /// Forwarded from the embedded SimilarStrip. The owning pane
    /// re-emits this so MainWindow can open the clicked item through
    /// the normal discover flow.
    void similarActivated(const api::DiscoverItem& item);

private:
    void updatePoster();

    ImageLoader* m_loader;

    QStackedWidget* m_stack {};  // state | content-scroll
    StateWidget* m_state {};
    QScrollArea* m_scroll {};

    QLabel* m_posterLabel {};
    QLabel* m_titleLabel {};
    QLabel* m_upcomingBadge {};
    QLabel* m_metaLineLabel {};
    QLabel* m_descLabel {};

    QUrl m_pendingPosterUrl;

    // Null when constructed without a TmdbClient (tests).
    SimilarStrip* m_similar {};
};

} // namespace kinema::ui
