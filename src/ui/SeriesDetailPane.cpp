// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/SeriesDetailPane.h"

#include "config/Config.h"
#include "core/StreamFilter.h"
#include "ui/ImageLoader.h"
#include "ui/SeriesPicker.h"
#include "ui/SimilarStrip.h"
#include "ui/StateWidget.h"
#include "ui/TorrentsModel.h"

#include <KLocalizedString>

#include <QAction>
#include <QCheckBox>
#include <QFrame>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QLabel>
#include <QMenu>
#include <QPixmap>
#include <QPixmapCache>
#include <QScrollArea>
#include <QSortFilterProxyModel>
#include <QSplitter>
#include <QStackedWidget>
#include <QTableView>
#include <QToolButton>
#include <QVBoxLayout>

namespace kinema::ui {

namespace {

/// Shared with DetailPane's SortProxy — sorts by TorrentsModel::SortKeyRole
/// when present, falls back to display text otherwise.
class SortProxy : public QSortFilterProxyModel
{
public:
    using QSortFilterProxyModel::QSortFilterProxyModel;

protected:
    bool lessThan(const QModelIndex& l, const QModelIndex& r) const override
    {
        auto lv = sourceModel()->data(l, TorrentsModel::SortKeyRole);
        auto rv = sourceModel()->data(r, TorrentsModel::SortKeyRole);
        if (lv.isValid() && rv.isValid()) {
            return lv.toLongLong() < rv.toLongLong();
        }
        return QSortFilterProxyModel::lessThan(l, r);
    }
};

} // namespace

SeriesDetailPane::SeriesDetailPane(ImageLoader* loader,
    api::TmdbClient* tmdb, QWidget* parent)
    : QWidget(parent)
    , m_loader(loader)
{
    if (tmdb) {
        m_similar = new SimilarStrip(tmdb, loader, this);
        connect(m_similar, &SimilarStrip::itemActivated,
            this, &SeriesDetailPane::similarActivated);
    }
    // ---- Close button (top-right of the pane) -----------------------------
    m_closeButton = new QToolButton(this);
    m_closeButton->setIcon(QIcon::fromTheme(QStringLiteral("window-close")));
    m_closeButton->setToolTip(i18nc("@info:tooltip", "Close details"));
    m_closeButton->setAutoRaise(true);
    connect(m_closeButton, &QToolButton::clicked,
        this, &SeriesDetailPane::closeRequested);

    // ---- Left column widgets ----------------------------------------------
    m_leftState = new StateWidget(this);

    m_posterLabel = new QLabel(this);
    m_posterLabel->setFixedSize(220, 330);
    m_posterLabel->setAlignment(Qt::AlignCenter);
    m_posterLabel->setFrameShape(QFrame::StyledPanel);

    m_titleLabel = new QLabel(this);
    {
        auto f = m_titleLabel->font();
        f.setPointSizeF(f.pointSizeF() * 1.4);
        f.setBold(true);
        m_titleLabel->setFont(f);
    }
    m_titleLabel->setWordWrap(true);

    m_metaLineLabel = new QLabel(this);
    m_metaLineLabel->setWordWrap(true);
    m_metaLineLabel->setTextFormat(Qt::PlainText);

    m_descLabel = new QLabel(this);
    m_descLabel->setWordWrap(true);
    m_descLabel->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    m_descLabel->setTextFormat(Qt::PlainText);

    auto* metaTextColumn = new QVBoxLayout;
    metaTextColumn->setContentsMargins(0, 0, 0, 0);
    metaTextColumn->setSpacing(6);
    metaTextColumn->addWidget(m_titleLabel);
    metaTextColumn->addWidget(m_metaLineLabel);
    metaTextColumn->addWidget(m_descLabel, 1);

    auto* metaRow = new QHBoxLayout;
    metaRow->setSpacing(12);
    metaRow->addWidget(m_posterLabel, 0, Qt::AlignTop);
    metaRow->addLayout(metaTextColumn, 1);

    auto* leftContent = new QWidget(this);
    auto* leftContentLayout = new QVBoxLayout(leftContent);
    leftContentLayout->setContentsMargins(12, 12, 12, 12);
    leftContentLayout->setSpacing(8);
    leftContentLayout->addLayout(metaRow);
    if (m_similar) {
        leftContentLayout->addWidget(m_similar, 0);
    }
    leftContentLayout->addStretch(1);

    m_leftScroll = new QScrollArea(this);
    m_leftScroll->setFrameShape(QFrame::NoFrame);
    m_leftScroll->setWidgetResizable(true);
    m_leftScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_leftScroll->setWidget(leftContent);

    m_leftStack = new QStackedWidget(this);
    m_leftStack->addWidget(m_leftState);   // idx 0 = state
    m_leftStack->addWidget(m_leftScroll);  // idx 1 = content

    // ---- Right column, page 0: SeriesPicker -------------------------------
    m_picker = new SeriesPicker(m_loader, this);
    connect(m_picker, &SeriesPicker::episodeActivated, this,
        [this](const api::Episode& ep) {
            // Flip the page immediately so the user sees feedback
            // before MainWindow's fetch roundtrip finishes.
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

    // ---- Right column, page 1: cached-only + torrents ---------------------
    m_cachedOnlyCheck = new QCheckBox(
        i18nc("@option:check", "Cached on Real-Debrid only"), this);
    m_cachedOnlyCheck->setChecked(config::Config::instance().cachedOnly());
    m_cachedOnlyCheck->setVisible(false);
    connect(m_cachedOnlyCheck, &QCheckBox::toggled, this,
        [this](bool on) {
            if (config::Config::instance().cachedOnly() != on) {
                config::Config::instance().setCachedOnly(on);
            }
            applyClientFilters();
        });
    connect(&config::Config::instance(), &config::Config::cachedOnlyChanged,
        this, [this](bool on) {
            if (m_cachedOnlyCheck->isChecked() != on) {
                const QSignalBlocker block(m_cachedOnlyCheck);
                m_cachedOnlyCheck->setChecked(on);
            }
            applyClientFilters();
        });
    connect(&config::Config::instance(), &config::Config::keywordBlocklistChanged,
        this, [this](const QStringList&) { applyClientFilters(); });

    m_torrents = new TorrentsModel(this);
    auto* proxy = new SortProxy(this);
    proxy->setSourceModel(m_torrents);

    m_torrentsView = new QTableView(this);
    m_torrentsView->setModel(proxy);
    m_torrentsView->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_torrentsView->setSelectionMode(QAbstractItemView::SingleSelection);
    m_torrentsView->setSortingEnabled(true);
    m_torrentsView->setAlternatingRowColors(true);
    m_torrentsView->verticalHeader()->setVisible(false);
    m_torrentsView->horizontalHeader()->setStretchLastSection(false);
    m_torrentsView->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColRelease, QHeaderView::Stretch);
    m_torrentsView->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColQuality, QHeaderView::ResizeToContents);
    m_torrentsView->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColSize, QHeaderView::ResizeToContents);
    m_torrentsView->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColSeeders, QHeaderView::ResizeToContents);
    m_torrentsView->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColProvider, QHeaderView::ResizeToContents);
    m_torrentsView->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_torrentsView, &QTableView::customContextMenuRequested,
        this, &SeriesDetailPane::onTorrentContextMenu);
    // Double-click / Enter on a row → Play, but only when we have a
    // direct URL (i.e. an RD-cached stream).
    connect(m_torrentsView, &QAbstractItemView::activated, this,
        [this, proxy](const QModelIndex& idx) {
            const auto src = proxy->mapToSource(idx);
            const auto* s = m_torrents->at(src.row());
            if (s && !s->directUrl.isEmpty()) {
                Q_EMIT playRequested(*s);
            }
        });

    m_torrentsState = new StateWidget(this);
    m_torrentsStack = new QStackedWidget(this);
    m_torrentsStack->addWidget(m_torrentsState);
    m_torrentsStack->addWidget(m_torrentsView);

    // Back button + breadcrumb header for the streams page.
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

    auto* cachedRow = new QHBoxLayout;
    cachedRow->setContentsMargins(0, 0, 0, 0);
    cachedRow->addWidget(m_cachedOnlyCheck);
    cachedRow->addStretch(1);

    auto* torrentsWrap = new QWidget(this);
    auto* torrentsLayout = new QVBoxLayout(torrentsWrap);
    torrentsLayout->setContentsMargins(6, 12, 12, 12);
    torrentsLayout->setSpacing(6);
    torrentsLayout->addLayout(streamsHeaderRow);
    torrentsLayout->addLayout(cachedRow);
    torrentsLayout->addWidget(m_torrentsStack, 1);

    // Right column hosts the two pages.
    m_rightStack = new QStackedWidget(this);
    m_rightStack->addWidget(pickerWrap);     // idx 0 = episodes
    m_rightStack->addWidget(torrentsWrap);   // idx 1 = streams
    m_rightStack->setCurrentIndex(0);

    // ---- Outer horizontal splitter (left column | right column) -----------
    m_split = new QSplitter(Qt::Horizontal, this);
    m_split->setChildrenCollapsible(false);
    m_split->addWidget(m_leftStack);
    m_split->addWidget(m_rightStack);
    m_split->setStretchFactor(0, 35);
    m_split->setStretchFactor(1, 65);
    const auto savedState = config::Config::instance().detailSplitterState();
    if (!savedState.isEmpty()) {
        m_split->restoreState(savedState);
    } else {
        m_split->setSizes({ 420, 780 });
    }
    connect(m_split, &QSplitter::splitterMoved, this,
        [this] { saveSplitterState(); });

    // ---- Root -------------------------------------------------------------
    auto* headerRow = new QHBoxLayout;
    headerRow->setContentsMargins(6, 6, 6, 0);
    headerRow->addStretch(1);
    headerRow->addWidget(m_closeButton, 0, Qt::AlignTop);

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(2);
    root->addLayout(headerRow, 0);
    root->addWidget(m_split, 1);

    // Repaint the poster when it finishes loading.
    if (m_loader) {
        connect(m_loader, &ImageLoader::posterReady, this,
            [this](const QUrl& url) {
                if (url == m_pendingPosterUrl) {
                    updatePoster();
                }
            });
    }

    showMetaLoading();
}

void SeriesDetailPane::setSimilarContext(const QString& imdbId)
{
    if (m_similar) {
        m_similar->setContextImdb(api::MediaKind::Series, imdbId);
    }
}

void SeriesDetailPane::saveSplitterState()
{
    config::Config::instance().setDetailSplitterState(m_split->saveState());
}

void SeriesDetailPane::setRealDebridConfigured(bool on)
{
    m_rdConfigured = on;
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void SeriesDetailPane::focusEpisodeList()
{
    // Delegate to the picker which owns the list view.
    m_picker->setFocus();
}

void SeriesDetailPane::showMetaLoading()
{
    m_leftState->showLoading(i18nc("@info:status", "Loading details…"));
    m_leftStack->setCurrentWidget(m_leftState);

    m_titleLabel->clear();
    m_metaLineLabel->clear();
    m_descLabel->clear();
    m_posterLabel->clear();
    m_pendingPosterUrl.clear();

    m_pickerState->showLoading(i18nc("@info:status", "Loading episodes…"));
    m_pickerStack->setCurrentWidget(m_pickerState);

    // Reset to the episodes page; clear any leftover stream state.
    m_rightStack->setCurrentIndex(0);
    m_episodeBreadcrumbLabel->clear();
    m_torrentsState->showIdle(QString {});
    m_torrentsStack->setCurrentWidget(m_torrentsState);

    m_rawStreams.clear();
    m_torrents->reset({});
    rebuildCachedOnlyVisibility();
}

void SeriesDetailPane::showMetaError(const QString& message)
{
    m_leftState->showError(message, /*retryable=*/false);
    m_leftStack->setCurrentWidget(m_leftState);

    m_pickerState->showError(message, /*retryable=*/false);
    m_pickerStack->setCurrentWidget(m_pickerState);

    m_rightStack->setCurrentIndex(0);
    m_episodeBreadcrumbLabel->clear();
    m_torrentsState->showIdle(QString {});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesDetailPane::setSeries(const api::SeriesDetail& series)
{
    const auto& sum = series.meta.summary;
    m_titleLabel->setText(sum.year
            ? QStringLiteral("%1 (%2)").arg(sum.title).arg(*sum.year)
            : sum.title);

    QStringList metaLine;
    if (!series.meta.genres.isEmpty()) {
        metaLine << series.meta.genres.join(QStringLiteral(", "));
    }
    if (series.meta.runtimeMinutes) {
        metaLine << i18nc("@label runtime", "%1 min", *series.meta.runtimeMinutes);
    }
    if (sum.imdbRating) {
        metaLine << i18nc("@label rating", "★ %1",
            QString::number(*sum.imdbRating, 'f', 1));
    }
    m_metaLineLabel->setText(metaLine.join(QStringLiteral("  ·  ")));
    m_descLabel->setText(sum.description);

    m_pendingPosterUrl = sum.poster;
    m_posterLabel->clear();
    updatePoster();

    m_leftStack->setCurrentWidget(m_leftScroll);

    m_picker->setSeries(series);
    m_pickerStack->setCurrentWidget(m_picker);

    // Fresh series load always lands on the episodes page.
    m_rightStack->setCurrentIndex(0);
    m_episodeBreadcrumbLabel->clear();
    m_torrentsState->showIdle(QString {});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
    m_rawStreams.clear();
    m_torrents->reset({});
    rebuildCachedOnlyVisibility();
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
    m_episodeBreadcrumbLabel->clear();
    m_torrentsState->showIdle(QString {});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
    m_rawStreams.clear();
    m_torrents->reset({});
    rebuildCachedOnlyVisibility();
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

void SeriesDetailPane::updatePoster()
{
    if (!m_loader || m_pendingPosterUrl.isEmpty()) {
        return;
    }
    auto task = m_loader->requestPoster(m_pendingPosterUrl);
    Q_UNUSED(task);
    QPixmap pm;
    const auto key = QStringLiteral("kinema:poster:")
        + m_pendingPosterUrl.toString(QUrl::FullyEncoded);
    if (QPixmapCache::find(key, &pm) && !pm.isNull()) {
        m_posterLabel->setPixmap(pm.scaled(m_posterLabel->size(),
            Qt::KeepAspectRatio, Qt::SmoothTransformation));
    }
}

void SeriesDetailPane::showTorrentsLoading()
{
    m_torrentsState->showLoading(i18nc("@info:status", "Finding torrents…"));
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesDetailPane::showTorrentsError(const QString& message)
{
    m_torrentsState->showError(message, /*retryable=*/false);
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesDetailPane::showTorrentsEmpty()
{
    m_torrentsState->showIdle(
        i18nc("@label", "No torrents found"),
        i18nc("@info", "Try a different episode or widen your filters."));
    m_torrents->reset({});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesDetailPane::setTorrents(QList<api::Stream> streams)
{
    m_rawStreams = std::move(streams);
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void SeriesDetailPane::applyClientFilters()
{
    if (m_rawStreams.isEmpty()) {
        m_torrents->reset({});
        showTorrentsEmpty();
        return;
    }

    core::stream_filter::ClientFilters filters;
    filters.cachedOnly = m_rdConfigured && m_cachedOnlyCheck->isChecked();
    filters.keywordBlocklist = config::Config::instance().keywordBlocklist();

    auto visible = core::stream_filter::apply(m_rawStreams, filters);

    if (visible.isEmpty()) {
        m_torrents->reset({});
        const QString title = filters.cachedOnly
            ? i18nc("@label", "No cached torrents")
            : i18nc("@label", "No torrents match your filters");
        const QString info = filters.cachedOnly
            ? i18nc("@info",
                "Uncheck \u201cCached on Real-Debrid only\u201d or widen "
                "your filters in Settings.")
            : i18nc("@info",
                "Loosen the exclusions or keyword blocklist in Settings.");
        m_torrentsState->showIdle(title, info);
        m_torrentsStack->setCurrentWidget(m_torrentsState);
        return;
    }
    m_torrents->reset(std::move(visible));
    m_torrentsView->sortByColumn(TorrentsModel::ColSeeders, Qt::DescendingOrder);
    m_torrentsStack->setCurrentWidget(m_torrentsView);
}

void SeriesDetailPane::rebuildCachedOnlyVisibility()
{
    m_cachedOnlyCheck->setVisible(m_rdConfigured && !m_rawStreams.isEmpty());
}

void SeriesDetailPane::onTorrentContextMenu(const QPoint& pos)
{
    const auto idx = m_torrentsView->indexAt(pos);
    if (!idx.isValid()) {
        return;
    }
    auto* proxy = qobject_cast<QSortFilterProxyModel*>(m_torrentsView->model());
    const auto srcIdx = proxy ? proxy->mapToSource(idx) : idx;
    const auto* s = m_torrents->at(srcIdx.row());
    if (!s) {
        return;
    }

    QMenu menu(this);
    auto* play = menu.addAction(
        QIcon::fromTheme(QStringLiteral("media-playback-start")),
        i18nc("@action:inmenu", "&Play"));
    menu.setDefaultAction(play);
    menu.addSeparator();
    auto* copyMagnet = menu.addAction(
        QIcon::fromTheme(QStringLiteral("edit-copy")),
        i18nc("@action:inmenu", "Copy magnet link"));
    auto* openMagnet = menu.addAction(
        QIcon::fromTheme(QStringLiteral("document-open")),
        i18nc("@action:inmenu", "Open magnet link"));
    menu.addSeparator();
    auto* copyDirect = menu.addAction(
        QIcon::fromTheme(QStringLiteral("edit-copy")),
        i18nc("@action:inmenu", "Copy direct URL"));
    auto* openDirect = menu.addAction(
        QIcon::fromTheme(QStringLiteral("document-open-remote")),
        i18nc("@action:inmenu", "Open direct URL"));

    const bool hasHash = !s->infoHash.isEmpty();
    const bool hasUrl = !s->directUrl.isEmpty();
    play->setEnabled(hasUrl);
    if (!hasUrl) {
        play->setToolTip(i18nc("@info:tooltip",
            "Direct playback needs a Real-Debrid cached stream."));
    }
    copyMagnet->setEnabled(hasHash);
    openMagnet->setEnabled(hasHash);
    copyDirect->setEnabled(hasUrl);
    openDirect->setEnabled(hasUrl);

    if (auto* chosen = menu.exec(m_torrentsView->viewport()->mapToGlobal(pos))) {
        if (chosen == play) {
            Q_EMIT playRequested(*s);
        } else if (chosen == copyMagnet) {
            Q_EMIT copyMagnetRequested(*s);
        } else if (chosen == openMagnet) {
            Q_EMIT openMagnetRequested(*s);
        } else if (chosen == copyDirect) {
            Q_EMIT copyDirectUrlRequested(*s);
        } else if (chosen == openDirect) {
            Q_EMIT openDirectUrlRequested(*s);
        }
    }
}

} // namespace kinema::ui
