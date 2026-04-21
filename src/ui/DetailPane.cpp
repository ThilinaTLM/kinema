// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/DetailPane.h"

#include "config/Config.h"
#include "core/StreamFilter.h"
#include "ui/ImageLoader.h"
#include "ui/SimilarStrip.h"
#include "ui/StateWidget.h"
#include "ui/TorrentsModel.h"

#include <KLocalizedString>

#include <QAction>
#include <QCheckBox>
#include <QCoro/QCoroTask>
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
#include <QVBoxLayout>

namespace kinema::ui {

namespace {

/// Simple proxy that sorts via the model's SortKeyRole when available.
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

DetailPane::DetailPane(ImageLoader* loader,
    api::TmdbClient* tmdb, QWidget* parent)
    : QWidget(parent)
    , m_loader(loader)
{
    if (tmdb) {
        m_similar = new SimilarStrip(tmdb, loader, this);
        connect(m_similar, &SimilarStrip::itemActivated,
            this, &DetailPane::similarActivated);
    }
    // ---- Left column widgets ----------------------------------------------
    m_metaState = new StateWidget(this);

    m_posterLabel = new QLabel(this);
    m_posterLabel->setFixedSize(220, 330);
    m_posterLabel->setAlignment(Qt::AlignCenter);
    m_posterLabel->setFrameShape(QFrame::StyledPanel);

    m_titleLabel = new QLabel(this);
    auto tf = m_titleLabel->font();
    tf.setPointSizeF(tf.pointSizeF() * 1.4);
    tf.setBold(true);
    m_titleLabel->setFont(tf);
    m_titleLabel->setWordWrap(true);

    m_metaLineLabel = new QLabel(this);
    m_metaLineLabel->setWordWrap(true);
    m_metaLineLabel->setTextFormat(Qt::PlainText);

    m_descLabel = new QLabel(this);
    m_descLabel->setWordWrap(true);
    m_descLabel->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    m_descLabel->setTextFormat(Qt::PlainText);

    auto* metaRight = new QVBoxLayout;
    metaRight->setSpacing(6);
    metaRight->addWidget(m_titleLabel);
    metaRight->addWidget(m_metaLineLabel);
    metaRight->addWidget(m_descLabel, 1);

    auto* metaRow = new QHBoxLayout;
    metaRow->setSpacing(12);
    metaRow->addWidget(m_posterLabel, 0, Qt::AlignTop);
    metaRow->addLayout(metaRight, 1);

    // Scrollable content column: poster+meta row, then description
    // (already inside metaRight), then the SimilarStrip at the bottom.
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
    m_leftStack->addWidget(m_metaState);    // idx 0 = state
    m_leftStack->addWidget(m_leftScroll);   // idx 1 = content

    // ---- Torrents side -----------------------------------------------------
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
        this, &DetailPane::onTorrentContextMenu);
    // Double-click / Enter on a row is treated as "Play" — but only when
    // a direct URL exists; otherwise we silently ignore so the user
    // isn't surprised by a no-op failure.
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

    // ---- Right column ------------------------------------------------------
    auto* cachedRow = new QHBoxLayout;
    cachedRow->setContentsMargins(0, 0, 0, 0);
    cachedRow->addWidget(m_cachedOnlyCheck);
    cachedRow->addStretch(1);

    auto* rightWrap = new QWidget(this);
    auto* rightLayout = new QVBoxLayout(rightWrap);
    rightLayout->setContentsMargins(6, 12, 12, 12);
    rightLayout->setSpacing(6);
    rightLayout->addLayout(cachedRow, 0);
    rightLayout->addWidget(m_torrentsStack, 1);

    // ---- Splitter ----------------------------------------------------------
    m_split = new QSplitter(Qt::Horizontal, this);
    m_split->setChildrenCollapsible(false);
    m_split->addWidget(m_leftStack);
    m_split->addWidget(rightWrap);
    m_split->setStretchFactor(0, 35);
    m_split->setStretchFactor(1, 65);
    const auto saved = config::Config::instance().detailSplitterState();
    if (!saved.isEmpty()) {
        m_split->restoreState(saved);
    } else {
        m_split->setSizes({ 420, 780 });
    }
    connect(m_split, &QSplitter::splitterMoved, this, [this] {
        config::Config::instance().setDetailSplitterState(m_split->saveState());
    });

    // ---- Root --------------------------------------------------------------
    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);
    root->addWidget(m_split, 1);

    showIdle();

    // When any poster arrives, see if it was the one we're waiting on.
    if (m_loader) {
        connect(m_loader, &ImageLoader::posterReady, this,
            [this](const QUrl& url) {
                if (url == m_pendingPosterUrl) {
                    updatePoster();
                }
            });
    }
}

void DetailPane::showIdle()
{
    m_metaState->showIdle(
        i18nc("@label", "Nothing selected"),
        i18nc("@info", "Pick a result to see details and torrents."));
    m_leftStack->setCurrentWidget(m_metaState);

    m_rawStreams.clear();
    m_torrents->reset({});
    m_torrentsState->showIdle(QString {});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
    rebuildCachedOnlyVisibility();

    if (m_similar) {
        m_similar->setContextImdb(api::MediaKind::Movie, QString {});
    }
}

void DetailPane::setSimilarContext(api::MediaKind kind, const QString& imdbId)
{
    if (m_similar) {
        m_similar->setContextImdb(kind, imdbId);
    }
}

void DetailPane::showMetaLoading()
{
    m_metaState->showLoading(i18nc("@info:status", "Loading details…"));
    m_leftStack->setCurrentWidget(m_metaState);
}

void DetailPane::showMetaError(const QString& message)
{
    m_metaState->showError(message, /*retryable=*/false);
    m_leftStack->setCurrentWidget(m_metaState);
}

void DetailPane::setMeta(const api::MetaDetail& meta)
{
    m_titleLabel->setText(meta.summary.year
            ? QStringLiteral("%1 (%2)").arg(meta.summary.title).arg(*meta.summary.year)
            : meta.summary.title);

    QStringList metaLine;
    if (!meta.genres.isEmpty()) {
        metaLine << meta.genres.join(QStringLiteral(", "));
    }
    if (meta.runtimeMinutes) {
        metaLine << i18nc("@label runtime", "%1 min", *meta.runtimeMinutes);
    }
    if (meta.summary.imdbRating) {
        metaLine << i18nc("@label rating", "★ %1",
            QString::number(*meta.summary.imdbRating, 'f', 1));
    }
    m_metaLineLabel->setText(metaLine.join(QStringLiteral("  ·  ")));

    m_descLabel->setText(meta.summary.description);

    m_pendingPosterUrl = meta.summary.poster;
    m_posterLabel->clear();
    updatePoster();

    m_leftStack->setCurrentWidget(m_leftScroll);
}

void DetailPane::setRealDebridConfigured(bool on)
{
    m_rdConfigured = on;
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void DetailPane::updatePoster()
{
    if (!m_loader || m_pendingPosterUrl.isEmpty()) {
        return;
    }
    // Kick off a (possibly cache-hitting) fetch, then set the pixmap in a
    // continuation. We don't await in this non-coroutine method.
    auto task = m_loader->requestPoster(m_pendingPosterUrl);
    Q_UNUSED(task);
    // Synchronous cache hit path: query QPixmapCache directly.
    QPixmap pm;
    const auto key = QStringLiteral("kinema:poster:")
        + m_pendingPosterUrl.toString(QUrl::FullyEncoded);
    if (QPixmapCache::find(key, &pm) && !pm.isNull()) {
        m_posterLabel->setPixmap(pm.scaled(m_posterLabel->size(),
            Qt::KeepAspectRatio, Qt::SmoothTransformation));
    }
}

void DetailPane::showTorrentsLoading()
{
    m_torrentsState->showLoading(i18nc("@info:status", "Finding torrents…"));
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void DetailPane::showTorrentsError(const QString& message)
{
    m_torrentsState->showError(message, /*retryable=*/false);
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void DetailPane::showTorrentsEmpty()
{
    m_torrentsState->showIdle(
        i18nc("@label", "No torrents found"),
        i18nc("@info", "Try a different release or widen your filters."));
    m_torrents->reset({});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void DetailPane::setTorrents(QList<api::Stream> streams)
{
    m_rawStreams = std::move(streams);
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void DetailPane::applyClientFilters()
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

void DetailPane::rebuildCachedOnlyVisibility()
{
    m_cachedOnlyCheck->setVisible(m_rdConfigured && !m_rawStreams.isEmpty());
}

void DetailPane::onTorrentContextMenu(const QPoint& pos)
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
