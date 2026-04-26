// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/SeriesDetailPane.h"

#include "config/AppearanceSettings.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "services/StreamActions.h"
#include "ui/SeriesPicker.h"
#include "ui/StateWidget.h"
#include "ui/widgets/MetaHeaderWidget.h"
#include "ui/widgets/StreamsPanel.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QSplitter>
#include <QStackedWidget>
#include <QToolButton>
#include <QVBoxLayout>

namespace kinema::ui {

SeriesDetailPane::SeriesDetailPane(ImageLoader* loader,
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
    connect(m_header, &MetaHeaderWidget::similarActivated,
        this, &SeriesDetailPane::similarActivated);

    // ---- Right column, page 0: SeriesPicker -----------------------------
    m_picker = new SeriesPicker(loader, this);
    connect(m_picker, &SeriesPicker::episodeActivated, this,
        [this](const api::Episode& ep) {
            // Flip immediately so the user sees feedback before the
            // controller's fetch roundtrip completes.
            showEpisodeStreamsPage(ep);
            Q_EMIT episodeSelected(ep);
        });

    m_pickerState = new StateWidget(this);
    m_pickerStack = new QStackedWidget(this);
    m_pickerStack->addWidget(m_pickerState);
    m_pickerStack->addWidget(m_picker);
    m_pickerStack->setCurrentWidget(m_pickerState);
    m_pickerState->showIdle(QString {});

    auto* pickerWrap = new QWidget(this);
    auto* pickerLayout = new QVBoxLayout(pickerWrap);
    pickerLayout->setContentsMargins(6, 12, 12, 12);
    pickerLayout->addWidget(m_pickerStack);

    // ---- Right column, page 1: breadcrumb + streams ---------------------
    m_streams = new StreamsPanel(torrentio, filter, actions, this);

    m_backToEpisodesButton = new QToolButton(this);
    m_backToEpisodesButton->setIcon(
        QIcon::fromTheme(QStringLiteral("go-previous")));
    m_backToEpisodesButton->setText(i18nc("@action:button", "Episodes"));
    m_backToEpisodesButton->setToolTip(
        i18nc("@info:tooltip", "Back to the episode list"));
    m_backToEpisodesButton->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_backToEpisodesButton->setAutoRaise(true);
    connect(m_backToEpisodesButton, &QToolButton::clicked, this, [this] {
        showEpisodesPage();
        Q_EMIT backToEpisodesRequested();
    });

    m_episodeBreadcrumbLabel = new QLabel(this);
    m_episodeBreadcrumbLabel->setTextFormat(Qt::PlainText);
    {
        auto f = m_episodeBreadcrumbLabel->font();
        f.setBold(true);
        m_episodeBreadcrumbLabel->setFont(f);
    }

    auto* streamsHeaderRow = new QHBoxLayout;
    streamsHeaderRow->setContentsMargins(0, 0, 0, 0);
    streamsHeaderRow->setSpacing(8);
    streamsHeaderRow->addWidget(m_backToEpisodesButton, 0);
    streamsHeaderRow->addWidget(m_episodeBreadcrumbLabel, 1);

    auto* streamsWrap = new QWidget(this);
    auto* streamsLayout = new QVBoxLayout(streamsWrap);
    streamsLayout->setContentsMargins(6, 12, 12, 12);
    streamsLayout->setSpacing(6);
    streamsLayout->addLayout(streamsHeaderRow);
    streamsLayout->addWidget(m_streams, 1);

    m_rightStack = new QStackedWidget(this);
    m_rightStack->addWidget(pickerWrap);   // idx 0 = episodes
    m_rightStack->addWidget(streamsWrap);  // idx 1 = streams
    m_rightStack->setCurrentIndex(0);

    // ---- Outer splitter --------------------------------------------------
    m_split = new QSplitter(Qt::Horizontal, this);
    m_split->setChildrenCollapsible(false);
    m_split->addWidget(m_header);
    m_split->addWidget(m_rightStack);
    m_split->setStretchFactor(0, 35);
    m_split->setStretchFactor(1, 65);
    const auto savedState = m_appearanceSettings.detailSplitterState();
    if (!savedState.isEmpty()) {
        m_split->restoreState(savedState);
    } else {
        m_split->setSizes({ 420, 780 });
    }
    connect(m_split, &QSplitter::splitterMoved, this,
        [this] { saveSplitterState(); });

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);
    root->addWidget(m_split, 1);

    showMetaLoading();
}

void SeriesDetailPane::setSimilarContext(const QString& imdbId)
{
    m_header->setSimilarContext(api::MediaKind::Series, imdbId);
}

void SeriesDetailPane::saveSplitterState()
{
    m_appearanceSettings.setDetailSplitterState(m_split->saveState());
}

void SeriesDetailPane::resetStreamsPage()
{
    m_episodeBreadcrumbLabel->clear();
    m_streams->setStreams({});
}

void SeriesDetailPane::setRealDebridConfigured(bool on)
{
    m_streams->setRealDebridConfigured(on);
}

void SeriesDetailPane::setPlaybackContext(const api::PlaybackContext& ctx)
{
    m_streams->setPlaybackContext(ctx);
}

const api::PlaybackContext& SeriesDetailPane::playbackContext() const
{
    return m_streams->playbackContext();
}

void SeriesDetailPane::focusEpisodeList()
{
    m_picker->setFocus();
}

void SeriesDetailPane::showMetaLoading()
{
    m_header->showLoading();

    m_pickerState->showLoading(i18nc("@info:status", "Loading episodes…"));
    m_pickerStack->setCurrentWidget(m_pickerState);

    m_rightStack->setCurrentIndex(0);
    resetStreamsPage();
}

void SeriesDetailPane::showMetaError(const QString& message)
{
    m_header->showError(message);

    m_pickerState->showError(message, /*retryable=*/false);
    m_pickerStack->setCurrentWidget(m_pickerState);

    m_rightStack->setCurrentIndex(0);
    resetStreamsPage();
}

void SeriesDetailPane::setSeries(const api::SeriesDetail& series)
{
    m_header->setMeta(series.meta);
    m_picker->setSeries(series);
    m_pickerStack->setCurrentWidget(m_picker);

    m_rightStack->setCurrentIndex(0);
    resetStreamsPage();
}

void SeriesDetailPane::showEpisodeStreamsPage(const api::Episode& ep)
{
    const QString crumb = ep.title.isEmpty()
        ? i18nc("@label season/episode short", "S%1E%2",
              ep.season, ep.number)
        : i18nc("@label season/episode short with title", "S%1E%2 · %3",
              ep.season, ep.number, ep.title);
    m_episodeBreadcrumbLabel->setText(crumb);
    m_rightStack->setCurrentIndex(1);
    m_backToEpisodesButton->setFocus();
}

void SeriesDetailPane::showEpisodesPage()
{
    m_rightStack->setCurrentIndex(0);
    resetStreamsPage();
    m_picker->setFocus();
}

bool SeriesDetailPane::tryPopStreamsPage()
{
    if (m_rightStack && m_rightStack->currentIndex() == 1) {
        showEpisodesPage();
        return true;
    }
    return false;
}

void SeriesDetailPane::showTorrentsUnreleased(const QDate& releaseDate)
{
    m_streams->showUnreleased(releaseDate);
}

void SeriesDetailPane::showTorrentsLoading()
{
    m_streams->showLoading();
}

void SeriesDetailPane::showTorrentsError(const QString& message)
{
    m_streams->showError(message);
}

void SeriesDetailPane::showTorrentsEmpty()
{
    m_streams->showEmpty();
}

void SeriesDetailPane::setTorrents(QList<api::Stream> streams)
{
    m_streams->setStreams(std::move(streams));
}

} // namespace kinema::ui
