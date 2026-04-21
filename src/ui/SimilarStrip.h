// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"

#include <QWidget>

#include <QCoro/QCoroTask>

class QModelIndex;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::ui {

class DiscoverSection;
class ImageLoader;

/**
 * "More like this" section for the movie and series detail panes.
 * Shown below the meta/description area; fed by TMDB recommendations
 * with a `/similar` fallback.
 *
 * Self-contained:
 *   - Call setContextImdb(kind, imdbId) when the pane loads a new
 *     item. The strip resolves IMDB \u2192 TMDB id via /find, then fetches
 *     recommendations. If recommendations are empty, it falls back to
 *     /similar. If both are empty, or TMDB is unconfigured, or the
 *     IMDB id is missing, the whole strip hides itself silently.
 *   - An internal epoch counter guards against stale loads when the
 *     user clicks rapidly through items.
 *
 * Visually, uses the same DiscoverSection as DiscoverPage: a titled
 * wrap-grid of cards, collapsed to a couple of rows by default with
 * a "Show more" toggle.
 *
 * Emits itemActivated(DiscoverItem). The owning pane forwards this to
 * MainWindow, which resolves the TMDB id of the clicked item back to
 * an IMDB id and opens the existing detail flow.
 */
class SimilarStrip : public QWidget
{
    Q_OBJECT
public:
    SimilarStrip(api::TmdbClient* tmdb, ImageLoader* images,
        QWidget* parent = nullptr);

    /// Trigger a fetch for the given (kind, IMDB id). Pass an empty
    /// IMDB id to clear the strip.
    void setContextImdb(api::MediaKind kind, QString imdbId);

Q_SIGNALS:
    void itemActivated(const api::DiscoverItem& item);

private:
    QCoro::Task<void> loadFor(api::MediaKind kind, QString imdbId);
    void onActivated(const QModelIndex& idx);

    api::TmdbClient* m_tmdb;
    ImageLoader* m_images;

    DiscoverSection* m_section {};

    quint64 m_epoch = 0;
    /// Cached last successful lookup so rapidly re-opening the same
    /// item doesn't re-hit /find + /recommendations. Keyed on
    /// "<kind>:<imdbId>" for simplicity.
    QString m_lastContextKey;
};

} // namespace kinema::ui
