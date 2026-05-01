// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/LibraryController.h"

#include "api/Media.h"
#include "core/LibraryStore.h"

#include <KLocalizedString>

#include <QDateTime>

namespace kinema::controllers {

namespace {

api::LibraryTitle titleFromMeta(const api::MetaDetail& meta,
    api::MediaKind kind)
{
    const auto& s = meta.summary;
    api::LibraryTitle t;
    t.kind = kind;
    t.imdbId = s.imdbId;
    t.title = s.title;
    t.year = s.year;
    t.poster = s.poster;
    t.backdrop = meta.background;
    t.overview = s.description;
    t.releaseDate = s.released;
    t.updatedAt = QDateTime::currentDateTimeUtc();
    return t;
}

api::LibraryEpisode episodeFromApi(const QString& seriesImdbId,
    const api::Episode& ep)
{
    api::LibraryEpisode out;
    out.seriesImdbId = seriesImdbId;
    out.season = ep.season;
    out.episode = ep.number;
    out.title = ep.title;
    out.overview = ep.description;
    out.thumbnail = ep.thumbnail;
    out.releaseDate = ep.released;
    out.updatedAt = QDateTime::currentDateTimeUtc();
    return out;
}

} // namespace

LibraryController::LibraryController(core::LibraryStore& store,
    QObject* parent)
    : QObject(parent)
    , m_store(store)
{
    connect(&m_store, &core::LibraryStore::changed,
        this, &LibraryController::changed);
}

LibraryController::~LibraryController() = default;

bool LibraryController::isInLibrary(api::MediaKind kind,
    const QString& imdbId) const
{
    return m_store.contains(kind, imdbId);
}

std::optional<api::LibraryTitle> LibraryController::title(
    api::MediaKind kind, const QString& imdbId) const
{
    return m_store.find(kind, imdbId);
}

QList<api::LibraryTitle> LibraryController::titles() const
{
    return m_store.titles();
}

QList<api::LibraryEpisode> LibraryController::episodesForSeries(
    const QString& imdbId) const
{
    return m_store.episodesForSeries(imdbId);
}

void LibraryController::saveMovie(const api::MetaDetail& meta)
{
    auto t = titleFromMeta(meta, api::MediaKind::Movie);
    if (t.imdbId.isEmpty() || t.title.isEmpty()) {
        return;
    }
    m_store.upsertTitle(t);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Added \u201c%1\u201d to Library.", t.title), 3000);
}

void LibraryController::saveSeries(const api::SeriesDetail& detail)
{
    auto t = titleFromMeta(detail.meta, api::MediaKind::Series);
    if (t.imdbId.isEmpty() || t.title.isEmpty()) {
        return;
    }
    m_store.upsertTitle(t);

    QList<api::LibraryEpisode> rows;
    rows.reserve(detail.episodes.size());
    for (const auto& ep : detail.episodes) {
        rows.append(episodeFromApi(t.imdbId, ep));
    }
    m_store.upsertEpisodes(t.imdbId, rows);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Added \u201c%1\u201d to Library.", t.title), 3000);
}

void LibraryController::removeFromLibrary(api::MediaKind kind,
    const QString& imdbId)
{
    const auto t = m_store.find(kind, imdbId);
    m_store.remove(kind, imdbId);
    if (t) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Removed \u201c%1\u201d from Library.", t->title),
            3000);
    }
}

} // namespace kinema::controllers
