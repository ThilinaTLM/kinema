// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/SeriesPicker.h"

#include "api/CinemetaParse.h"
#include "ui/EpisodeDelegate.h"
#include "ui/EpisodesModel.h"

#include <KLocalizedString>

#include <QComboBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QListView>
#include <QVBoxLayout>

namespace kinema::ui {

SeriesPicker::SeriesPicker(ImageLoader* loader, QWidget* parent)
    : QWidget(parent)
{
    auto* header = new QHBoxLayout;
    header->setContentsMargins(0, 0, 0, 0);
    header->setSpacing(6);

    auto* seasonLabel = new QLabel(i18nc("@label:listbox", "Season:"), this);
    m_seasonCombo = new QComboBox(this);
    m_seasonCombo->setMinimumWidth(80);

    header->addWidget(seasonLabel);
    header->addWidget(m_seasonCombo);
    header->addStretch(1);

    m_episodesModel = new EpisodesModel(this);

    m_episodeList = new QListView(this);
    m_episodeList->setModel(m_episodesModel);
    m_episodeList->setUniformItemSizes(true);
    m_episodeList->setSelectionMode(QAbstractItemView::SingleSelection);
    m_episodeList->setAlternatingRowColors(true);
    m_episodeList->setMouseTracking(true);
    // Rows always fill the viewport; horizontal scrolling would only
    // ever show up as a glitch from stale uniform-item-size caching.
    m_episodeList->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);

    m_episodeDelegate = new EpisodeDelegate(loader, m_episodeList);
    m_episodeList->setItemDelegate(m_episodeDelegate);

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(6);
    layout->addLayout(header);
    layout->addWidget(m_episodeList, 1);

    connect(m_seasonCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
        this, &SeriesPicker::onSeasonIndexChanged);
    connect(m_episodeList, &QListView::activated,
        this, &SeriesPicker::onEpisodeActivated);
    connect(m_episodeList, &QListView::clicked,
        this, &SeriesPicker::onEpisodeActivated);
}

void SeriesPicker::clear()
{
    m_allEpisodes.clear();
    m_seasons.clear();
    m_currentSeason = 0;

    const QSignalBlocker block(m_seasonCombo);
    m_seasonCombo->clear();
    m_episodesModel->reset({});
    m_episodeDelegate->resetRequestTracking();
}

void SeriesPicker::setSeries(const api::SeriesDetail& series)
{
    m_imdbId = series.meta.summary.imdbId;
    m_allEpisodes = series.episodes;
    m_seasons = api::cinemeta::seasonNumbers(m_allEpisodes);
    m_episodeDelegate->resetRequestTracking();

    const QSignalBlocker block(m_seasonCombo);
    m_seasonCombo->clear();
    for (int s : m_seasons) {
        m_seasonCombo->addItem(i18nc("@item:inlistbox season number", "Season %1", s), s);
    }

    if (m_seasons.isEmpty()) {
        m_currentSeason = 0;
        m_episodesModel->reset({});
        return;
    }

    m_seasonCombo->setCurrentIndex(0);
    m_currentSeason = m_seasons.first();
    rebuildEpisodesForSeason(m_currentSeason);
    if (m_episodesModel->rowCount() > 0) {
        m_episodeList->setCurrentIndex(m_episodesModel->index(0, 0));
    }
}

void SeriesPicker::selectEpisode(int season, int number)
{
    // Ensure the right season is displayed.
    const int seasonIdx = m_seasons.indexOf(season);
    if (seasonIdx < 0) {
        return;
    }
    if (m_seasonCombo->currentIndex() != seasonIdx) {
        m_seasonCombo->setCurrentIndex(seasonIdx);
    }
    for (int row = 0; row < m_episodesModel->rowCount(); ++row) {
        const auto* e = m_episodesModel->at(row);
        if (e && e->season == season && e->number == number) {
            m_episodeList->setCurrentIndex(m_episodesModel->index(row, 0));
            return;
        }
    }
}

void SeriesPicker::onSeasonIndexChanged(int idx)
{
    if (idx < 0 || idx >= m_seasons.size()) {
        return;
    }
    const int season = m_seasons.at(idx);
    if (season == m_currentSeason) {
        return;
    }
    m_currentSeason = season;
    rebuildEpisodesForSeason(season);
    if (m_episodesModel->rowCount() > 0) {
        m_episodeList->setCurrentIndex(m_episodesModel->index(0, 0));
    }
    Q_EMIT seasonChanged(season);
}

void SeriesPicker::rebuildEpisodesForSeason(int season)
{
    QList<api::Episode> forSeason;
    for (const auto& e : m_allEpisodes) {
        if (e.season == season) {
            forSeason.append(e);
        }
    }
    m_episodeDelegate->resetRequestTracking();
    m_episodesModel->reset(std::move(forSeason));
}

void SeriesPicker::onEpisodeActivated(const QModelIndex& idx)
{
    if (!idx.isValid()) {
        return;
    }
    const auto* e = m_episodesModel->at(idx.row());
    if (!e) {
        return;
    }
    Q_EMIT episodeActivated(*e);
}

} // namespace kinema::ui
