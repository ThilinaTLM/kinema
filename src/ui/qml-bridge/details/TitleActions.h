// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QCoro/QCoroTask>

#include <QObject>
#include <QString>

namespace kinema::api {
class TmdbClient;
}

namespace kinema::controllers {
class LibraryController;
class WatchedController;
}

namespace kinema::ui::qml::title_actions {

/**
 * Free-function helpers shared by every list / rail view-model
 * that exposes the canonical "row context menu" (Discover, Browse,
 * Search, Similar). They resolve the TMDB id to an IMDb id and
 * delegate to the matching controller; the caller's only job is
 * to plumb the row's `(tmdbId, kind, title)` triple through.
 *
 * `statusSink` is the view-model that should surface error /
 * progress messages via its `statusMessage(QString, int)` signal.
 * Passed as a `QObject*` to keep this header agnostic of the
 * concrete VM type; the helper invokes the signal by name.
 *
 * Each task captures everything by value and is safe to discard
 * (`Q_UNUSED(task)`); errors are translated through
 * `core::describeError` and reported on `statusSink`.
 *
 * Search / detail-page rows already carry an IMDb id and call
 * the controller directly \u2014 they don't need these helpers.
 */

/// Resolve `tmdbId` to an IMDb id and add the result to the
/// library via `LibraryController::saveByImdbId`. Idempotent.
QCoro::Task<void> addToLibraryByTmdb(api::TmdbClient* tmdb,
    controllers::LibraryController* library,
    QObject* statusSink,
    int tmdbId, domain::MediaKind kind, QString title);

/// Resolve `tmdbId` to an IMDb id and mark the movie as watched
/// via `WatchedController::setMovieWatched`. No-op for series
/// rows (per-episode watched state belongs on the detail page).
/// `title` is used for status feedback only.
QCoro::Task<void> markWatchedByTmdb(api::TmdbClient* tmdb,
    controllers::WatchedController* watched,
    QObject* statusSink,
    int tmdbId, domain::MediaKind kind, QString title);

} // namespace kinema::ui::qml::title_actions
