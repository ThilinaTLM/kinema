// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/SeriesFocusView.h"

#include "config/Config.h"
#include "core/StreamFilter.h"
#include "ui/ImageLoader.h"
#include "ui/SeriesPicker.h"
#include "ui/StateWidget.h"
#include "ui/TorrentsModel.h"

#include <KLocalizedString>

#include <QAction>
#include <QCheckBox>
#include <QFrame>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QKeySequence>
#include <QLabel>
#include <QMenu>
#include <QPixmap>
#include <QPixmapCache>
#include <QPushButton>
#include <QShortcut>
#include <QSortFilterProxyModel>
#include <QSplitter>
#include <QStackedWidget>
#include <QTableView>
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

SeriesFocusView::SeriesFocusView(ImageLoader* loader, QWidget* parent)
    : QWidget(parent)
    , m_loader(loader)
{
    // ---- Header strip ------------------------------------------------------
    m_backButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("go-previous")),
        i18nc("@action:button", "Back to results"), this);
    m_backButton->setAutoDefault(false);
    m_backButton->setDefault(false);
    m_backButton->setShortcut(QKeySequence(Qt::ALT | Qt::Key_Left));
    connect(m_backButton, &QPushButton::clicked,
        this, &SeriesFocusView::backRequested);

    // Esc also triggers back. Use a widget-wide shortcut so the episode
    // list's own Qt::Key_Escape doesn't swallow it.
    auto* escShortcut = new QShortcut(QKeySequence(Qt::Key_Escape), this);
    escShortcut->setContext(Qt::WidgetWithChildrenShortcut);
    connect(escShortcut, &QShortcut::activated,
        this, &SeriesFocusView::backRequested);

    m_posterLabel = new QLabel(this);
    m_posterLabel->setFixedSize(120, 180);
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
    m_descLabel->setMaximumHeight(80);

    auto* textColumn = new QVBoxLayout;
    textColumn->setContentsMargins(0, 0, 0, 0);
    textColumn->setSpacing(4);
    textColumn->addWidget(m_titleLabel);
    textColumn->addWidget(m_metaLineLabel);
    textColumn->addWidget(m_descLabel, 1);

    auto* headerRow = new QHBoxLayout;
    headerRow->setContentsMargins(12, 12, 12, 12);
    headerRow->setSpacing(12);
    headerRow->addWidget(m_posterLabel, 0, Qt::AlignTop);
    headerRow->addLayout(textColumn, 1);

    auto* headerTop = new QHBoxLayout;
    headerTop->setContentsMargins(12, 8, 12, 0);
    headerTop->addWidget(m_backButton);
    headerTop->addStretch(1);

    auto* headerWrap = new QWidget(this);
    auto* headerVBox = new QVBoxLayout(headerWrap);
    headerVBox->setContentsMargins(0, 0, 0, 0);
    headerVBox->setSpacing(0);
    headerVBox->addLayout(headerTop);
    headerVBox->addLayout(headerRow);

    // ---- Top half of splitter: SeriesPicker -------------------------------
    m_picker = new SeriesPicker(m_loader, this);
    connect(m_picker, &SeriesPicker::episodeActivated,
        this, &SeriesFocusView::episodeSelected);

    m_pickerState = new StateWidget(this);
    m_pickerStack = new QStackedWidget(this);
    m_pickerStack->addWidget(m_pickerState);
    m_pickerStack->addWidget(m_picker);
    m_pickerStack->setCurrentWidget(m_pickerState);
    m_pickerState->showIdle(QString {});

    auto* pickerWrap = new QWidget(this);
    auto* pickerLayout = new QVBoxLayout(pickerWrap);
    pickerLayout->setContentsMargins(12, 4, 12, 4);
    pickerLayout->addWidget(m_pickerStack);

    // ---- Bottom half of splitter: cached-only + torrents -------------------
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
        this, &SeriesFocusView::onTorrentContextMenu);
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

    auto* cachedRow = new QHBoxLayout;
    cachedRow->setContentsMargins(0, 0, 0, 0);
    cachedRow->addWidget(m_cachedOnlyCheck);
    cachedRow->addStretch(1);

    auto* torrentsWrap = new QWidget(this);
    auto* torrentsLayout = new QVBoxLayout(torrentsWrap);
    torrentsLayout->setContentsMargins(12, 4, 12, 8);
    torrentsLayout->setSpacing(4);
    torrentsLayout->addLayout(cachedRow);
    torrentsLayout->addWidget(m_torrentsStack, 1);

    // ---- Splitter ---------------------------------------------------------
    m_bodySplit = new QSplitter(Qt::Vertical, this);
    m_bodySplit->setChildrenCollapsible(false);
    m_bodySplit->addWidget(pickerWrap);
    m_bodySplit->addWidget(torrentsWrap);
    // Default 40/60 episodes/torrents; may be overridden by saved state.
    applyDefaultSplitterRatio();

    const auto savedState = config::Config::instance().focusSplitterState();
    if (!savedState.isEmpty()) {
        m_bodySplit->restoreState(savedState);
    }
    connect(m_bodySplit, &QSplitter::splitterMoved, this,
        [this] { saveSplitterState(); });

    // ---- Root -------------------------------------------------------------
    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);
    root->addWidget(headerWrap, 0);
    root->addWidget(m_bodySplit, 1);

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

void SeriesFocusView::applyDefaultSplitterRatio()
{
    // 40% top, 60% bottom. Uses a ratio because the splitter may be
    // asked for sizes before the widget has its real geometry.
    m_bodySplit->setSizes({ 400, 600 });
    m_bodySplit->setStretchFactor(0, 4);
    m_bodySplit->setStretchFactor(1, 6);
}

void SeriesFocusView::saveSplitterState()
{
    config::Config::instance().setFocusSplitterState(m_bodySplit->saveState());
}

void SeriesFocusView::setRealDebridConfigured(bool on)
{
    m_rdConfigured = on;
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void SeriesFocusView::focusEpisodeList()
{
    // Delegate to the picker which owns the list view.
    m_picker->setFocus();
}

void SeriesFocusView::showMetaLoading()
{
    m_titleLabel->setText(i18nc("@info:status", "Loading…"));
    m_metaLineLabel->clear();
    m_descLabel->clear();
    m_posterLabel->clear();
    m_pendingPosterUrl.clear();

    m_pickerState->showLoading(i18nc("@info:status", "Loading episodes…"));
    m_pickerStack->setCurrentWidget(m_pickerState);

    m_torrentsState->showIdle(QString {});
    m_torrentsStack->setCurrentWidget(m_torrentsState);

    m_rawStreams.clear();
    m_torrents->reset({});
    rebuildCachedOnlyVisibility();
}

void SeriesFocusView::showMetaError(const QString& message)
{
    m_titleLabel->setText(i18nc("@label", "Couldn't load series"));
    m_metaLineLabel->clear();
    m_descLabel->setText(message);
    m_posterLabel->clear();

    m_pickerState->showError(message, /*retryable=*/false);
    m_pickerStack->setCurrentWidget(m_pickerState);

    m_torrentsState->showIdle(QString {});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesFocusView::setSeries(const api::SeriesDetail& series)
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

    m_picker->setSeries(series);
    m_pickerStack->setCurrentWidget(m_picker);
}

void SeriesFocusView::updatePoster()
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

void SeriesFocusView::showTorrentsLoading()
{
    m_torrentsState->showLoading(i18nc("@info:status", "Finding torrents…"));
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesFocusView::showTorrentsError(const QString& message)
{
    m_torrentsState->showError(message, /*retryable=*/false);
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesFocusView::showTorrentsEmpty()
{
    m_torrentsState->showIdle(
        i18nc("@label", "No torrents found"),
        i18nc("@info", "Try a different episode or widen your filters."));
    m_torrents->reset({});
    m_torrentsStack->setCurrentWidget(m_torrentsState);
}

void SeriesFocusView::setTorrents(QList<api::Stream> streams)
{
    m_rawStreams = std::move(streams);
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void SeriesFocusView::applyClientFilters()
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

void SeriesFocusView::rebuildCachedOnlyVisibility()
{
    m_cachedOnlyCheck->setVisible(m_rdConfigured && !m_rawStreams.isEmpty());
}

void SeriesFocusView::onTorrentContextMenu(const QPoint& pos)
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
