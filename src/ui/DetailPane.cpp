// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/DetailPane.h"

#include "config/AppearanceSettings.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "services/StreamActions.h"
#include "ui/TorrentsModel.h"
#include "ui/widgets/MetaHeaderWidget.h"
#include "ui/widgets/StreamsPanel.h"

#include <QSplitter>
#include <QVBoxLayout>

namespace kinema::ui {

DetailPane::DetailPane(ImageLoader* loader,
    api::TmdbClient* tmdb,
    config::TorrentioSettings& torrentio,
    config::FilterSettings& filter,
    services::StreamActions& actions,
    config::AppearanceSettings& appearance,
    QWidget* parent)
    : QWidget(parent)
    , m_appearanceSettings(appearance)
{
    m_header = new MetaHeaderWidget(loader, tmdb, this);
    m_streams = new StreamsPanel(torrentio, filter, actions, this);
    connect(m_header, &MetaHeaderWidget::similarActivated,
        this, &DetailPane::similarActivated);

    auto* rightWrap = new QWidget(this);
    auto* rightLayout = new QVBoxLayout(rightWrap);
    rightLayout->setContentsMargins(6, 12, 12, 12);
    rightLayout->setSpacing(0);
    rightLayout->addWidget(m_streams, 1);

    m_split = new QSplitter(Qt::Horizontal, this);
    m_split->setChildrenCollapsible(false);
    m_split->addWidget(m_header);
    m_split->addWidget(rightWrap);
    m_split->setStretchFactor(0, 35);
    m_split->setStretchFactor(1, 65);
    const auto saved = m_appearanceSettings.detailSplitterState();
    if (!saved.isEmpty()) {
        m_split->restoreState(saved);
    } else {
        m_split->setSizes({ 420, 780 });
    }
    connect(m_split, &QSplitter::splitterMoved, this, [this] {
        m_appearanceSettings.setDetailSplitterState(m_split->saveState());
    });

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);
    root->addWidget(m_split, 1);

    showIdle();
}

void DetailPane::showIdle()
{
    m_header->showIdle();
    m_streams->setStreams({});
    m_header->setSimilarContext(api::MediaKind::Movie, QString {});
}

void DetailPane::showMetaLoading()
{
    m_header->showLoading();
}

void DetailPane::showMetaError(const QString& message)
{
    m_header->showError(message);
}

void DetailPane::setMeta(const api::MetaDetail& meta)
{
    m_header->setMeta(meta);
}

void DetailPane::setSimilarContext(api::MediaKind kind, const QString& imdbId)
{
    m_header->setSimilarContext(kind, imdbId);
}

void DetailPane::showTorrentsLoading()
{
    m_streams->showLoading();
}

void DetailPane::showTorrentsError(const QString& message)
{
    m_streams->showError(message);
}

void DetailPane::setTorrents(QList<api::Stream> streams)
{
    m_streams->setStreams(std::move(streams));
}

void DetailPane::showTorrentsEmpty()
{
    m_streams->showEmpty();
}

void DetailPane::showTorrentsUnreleased(const QDate& releaseDate)
{
    m_streams->showUnreleased(releaseDate);
}

void DetailPane::setRealDebridConfigured(bool on)
{
    m_streams->setRealDebridConfigured(on);
}

TorrentsModel* DetailPane::torrentsModel() const
{
    return m_streams->model();
}

} // namespace kinema::ui
