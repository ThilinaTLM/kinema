// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"

#include <QWidget>

class QComboBox;
class QListView;

namespace kinema::ui {

class EpisodeDelegate;
class EpisodesModel;
class ImageLoader;

/**
 * Season + episode picker, shown by DetailPane when the current item is
 * a series.
 *
 *   Season: [ 1  ▾ ]
 *   ┌───────┬──────────────────┐
 *   │ thumb │ 1x01  Pilot      │
 *   │       │ Jan 20, 2008     │
 *   └───────┴──────────────────┘
 *   …
 *
 * Emits `episodeActivated` on single click or Enter. Emits
 * `seasonChanged` when the user picks a different season. The owning
 * widget is expected to auto-select S1E1 after `setSeries()`.
 */
class SeriesPicker : public QWidget
{
    Q_OBJECT
public:
    explicit SeriesPicker(ImageLoader* loader, QWidget* parent = nullptr);

    void setSeries(const api::SeriesDetail& series);
    void clear();

    /// Currently-selected season. 0 if no series is loaded.
    int currentSeason() const { return m_currentSeason; }

    /// Selects the given (season, episode) if present. Silently no-op otherwise.
    void selectEpisode(int season, int number);

Q_SIGNALS:
    void seasonChanged(int season);
    void episodeActivated(const api::Episode& ep);

private:
    void onSeasonIndexChanged(int idx);
    void onEpisodeActivated(const QModelIndex& idx);
    void rebuildEpisodesForSeason(int season);

    QString m_imdbId;
    QList<api::Episode> m_allEpisodes;
    QList<int> m_seasons;
    int m_currentSeason = 0;

    QComboBox* m_seasonCombo {};
    QListView* m_episodeList {};
    EpisodesModel* m_episodesModel {};
    EpisodeDelegate* m_episodeDelegate {};
};

} // namespace kinema::ui
