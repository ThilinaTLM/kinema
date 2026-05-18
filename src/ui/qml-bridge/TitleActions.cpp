// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/TitleActions.h"

#include "api/TmdbClient.h"
#include "controllers/LibraryController.h"
#include "controllers/WatchedController.h"
#include "core/io/HttpErrorPresenter.h"

#include <KLocalizedString>

#include <QMetaObject>
#include <QPointer>

#include <exception>
#include <utility>

namespace kinema::ui::qml::title_actions {

namespace {

void emitStatus(QObject* sink, QString text, int durationMs)
{
    if (!sink || text.isEmpty()) {
        return;
    }
    // Each VM declares its own `statusMessage(QString, int)` signal
    // (the type returned by Q_SIGNALS is fixed-shape across all
    // VMs). Using invokeMethod with the registered signal name lets
    // this helper stay agnostic of the concrete VM type.
    QMetaObject::invokeMethod(sink, "statusMessage",
        Qt::DirectConnection,
        Q_ARG(QString, text), Q_ARG(int, durationMs));
}

QCoro::Task<QString> resolveImdbId(api::TmdbClient* tmdb,
    int tmdbId, domain::MediaKind kind)
{
    if (kind == domain::MediaKind::Series) {
        co_return co_await tmdb->imdbIdForTmdbSeries(tmdbId);
    }
    co_return co_await tmdb->imdbIdForTmdbMovie(tmdbId);
}

} // namespace

QCoro::Task<void> addToLibraryByTmdb(api::TmdbClient* tmdb,
    controllers::LibraryController* library,
    QObject* statusSink,
    int tmdbId, domain::MediaKind kind, QString title)
{
    if (!tmdb || !library || tmdbId <= 0) {
        co_return;
    }
    QPointer<QObject> sink(statusSink);
    QPointer<controllers::LibraryController> lib(library);
    try {
        const QString imdbId = co_await resolveImdbId(tmdb, tmdbId, kind);
        if (imdbId.isEmpty()) {
            emitStatus(sink,
                i18nc("@info:status",
                    "Could not resolve IMDb id for \u201c%1\u201d.",
                    title.isEmpty() ? QString::number(tmdbId) : title),
                5000);
            co_return;
        }
        if (!lib) {
            co_return;
        }
        // LibraryController owns the user-facing toast for the
        // "added" / "already in library" outcomes.
        co_await lib->saveByImdbId(imdbId, kind);
    } catch (const std::exception& e) {
        emitStatus(sink,
            core::describeError(e, "add to library"), 5000);
    }
    co_return;
}

QCoro::Task<void> markWatchedByTmdb(api::TmdbClient* tmdb,
    controllers::WatchedController* watched,
    QObject* statusSink,
    int tmdbId, domain::MediaKind kind, QString title)
{
    if (!tmdb || !watched || tmdbId <= 0) {
        co_return;
    }
    if (kind == domain::MediaKind::Series) {
        // Per-episode watched state lives on the detail page;
        // marking an entire series watched from a poster is too
        // coarse to be useful. The menu surfaces this no-op as a
        // hint rather than a confusing acknowledgement.
        emitStatus(statusSink,
            i18nc("@info:status",
                "Open \u201c%1\u201d to mark individual episodes "
                "as watched.",
                title.isEmpty() ? QString::number(tmdbId) : title),
            4000);
        co_return;
    }
    QPointer<QObject> sink(statusSink);
    QPointer<controllers::WatchedController> w(watched);
    try {
        const QString imdbId = co_await resolveImdbId(tmdb, tmdbId, kind);
        if (imdbId.isEmpty() || !w) {
            co_return;
        }
        w->setMovieWatched(imdbId, /*watched=*/true);
        emitStatus(sink,
            i18nc("@info:status",
                "Marked \u201c%1\u201d as watched.",
                title.isEmpty() ? imdbId : title),
            3000);
    } catch (const std::exception& e) {
        emitStatus(sink,
            core::describeError(e, "mark watched"), 5000);
    }
    co_return;
}

} // namespace kinema::ui::qml::title_actions
