// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/StreamsPanel.h"

#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "core/DateFormat.h"
#include "core/StreamFilter.h"
#include "services/StreamActions.h"
#include "ui/StateWidget.h"
#include "ui/TorrentsModel.h"

#include <KLocalizedString>

#include <QAction>
#include <QCheckBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QMenu>
#include <QSignalBlocker>
#include <QSortFilterProxyModel>
#include <QStackedWidget>
#include <QTableView>
#include <QVBoxLayout>

namespace kinema::ui {

namespace {

/// Sort via TorrentsModel::SortKeyRole when available, otherwise
/// fall back to display-text comparison. Shared in spirit with the
/// old DetailPane / SeriesDetailPane private SortProxy; lives here
/// now that streams are one widget.
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

StreamsPanel::StreamsPanel(config::TorrentioSettings& torrentio,
    config::FilterSettings& filters,
    services::StreamActions& actions,
    QWidget* parent)
    : QWidget(parent)
    , m_torrentioSettings(torrentio)
    , m_filterSettings(filters)
    , m_actions(actions)
{
    // ---- Cached-only checkbox (two-way bound to TorrentioSettings) ---------
    m_cachedOnlyCheck = new QCheckBox(
        i18nc("@option:check", "Cached on Real-Debrid only"), this);
    m_cachedOnlyCheck->setChecked(m_torrentioSettings.cachedOnly());
    m_cachedOnlyCheck->setVisible(false);
    connect(m_cachedOnlyCheck, &QCheckBox::toggled, this,
        [this](bool on) {
            if (m_torrentioSettings.cachedOnly() != on) {
                m_torrentioSettings.setCachedOnly(on);
            }
            applyClientFilters();
        });
    connect(&m_torrentioSettings,
        &config::TorrentioSettings::cachedOnlyChanged,
        this, [this](bool on) {
            if (m_cachedOnlyCheck->isChecked() != on) {
                const QSignalBlocker block(m_cachedOnlyCheck);
                m_cachedOnlyCheck->setChecked(on);
            }
            applyClientFilters();
        });
    connect(&m_filterSettings,
        &config::FilterSettings::keywordBlocklistChanged,
        this, [this](const QStringList&) { applyClientFilters(); });

    // ---- Torrents view + state -------------------------------------------
    m_torrents = new TorrentsModel(this);
    auto* proxy = new SortProxy(this);
    proxy->setSourceModel(m_torrents);

    m_view = new QTableView(this);
    m_view->setModel(proxy);
    m_view->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setSortingEnabled(true);
    m_view->setAlternatingRowColors(true);
    m_view->verticalHeader()->setVisible(false);
    m_view->horizontalHeader()->setStretchLastSection(false);
    m_view->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColRelease, QHeaderView::Stretch);
    m_view->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColQuality, QHeaderView::ResizeToContents);
    m_view->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColSize, QHeaderView::ResizeToContents);
    m_view->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColSeeders, QHeaderView::ResizeToContents);
    m_view->horizontalHeader()->setSectionResizeMode(
        TorrentsModel::ColProvider, QHeaderView::ResizeToContents);
    m_view->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_view, &QTableView::customContextMenuRequested,
        this, &StreamsPanel::onTorrentContextMenu);

    // Double-click / Enter on a row → Play (only when RD resolved a
    // direct URL; otherwise silently ignored so a no-op isn't
    // surprising).
    connect(m_view, &QAbstractItemView::activated, this,
        [this, proxy](const QModelIndex& idx) {
            const auto src = proxy->mapToSource(idx);
            const auto* s = m_torrents->at(src.row());
            if (s && !s->directUrl.isEmpty()) {
                m_actions.play(*s);
            }
        });

    m_state = new StateWidget(this);

    m_stack = new QStackedWidget(this);
    m_stack->addWidget(m_state);
    m_stack->addWidget(m_view);

    // ---- Layout ----------------------------------------------------------
    auto* cachedRow = new QHBoxLayout;
    cachedRow->setContentsMargins(0, 0, 0, 0);
    cachedRow->addWidget(m_cachedOnlyCheck);
    cachedRow->addStretch(1);

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(6);
    root->addLayout(cachedRow, 0);
    root->addWidget(m_stack, 1);
}

void StreamsPanel::showLoading()
{
    m_state->showLoading(i18nc("@info:status", "Finding torrents\u2026"));
    m_stack->setCurrentWidget(m_state);
}

void StreamsPanel::showError(const QString& message)
{
    m_state->showError(message, /*retryable=*/false);
    m_stack->setCurrentWidget(m_state);
}

void StreamsPanel::showEmpty()
{
    m_state->showIdle(
        i18nc("@label", "No torrents found"),
        i18nc("@info", "Try a different release or widen your filters."));
    m_torrents->reset({});
    m_stack->setCurrentWidget(m_state);
}

void StreamsPanel::showUnreleased(const QDate& releaseDate)
{
    m_state->showIdle(
        i18nc("@info:status when a title has not been released yet",
            "Not released yet"),
        i18nc("@info:status body. %1 is a formatted date",
            "Streams will be available from %1.",
            core::formatReleaseDate(releaseDate)));
    m_stack->setCurrentWidget(m_state);
    m_rawStreams.clear();
    m_torrents->reset({});
    rebuildCachedOnlyVisibility();
}

void StreamsPanel::setStreams(QList<api::Stream> streams)
{
    m_rawStreams = std::move(streams);
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void StreamsPanel::setRealDebridConfigured(bool on)
{
    m_rdConfigured = on;
    rebuildCachedOnlyVisibility();
    applyClientFilters();
}

void StreamsPanel::applyClientFilters()
{
    if (m_rawStreams.isEmpty()) {
        m_torrents->reset({});
        // Show an empty state rather than an empty table.
        m_state->showIdle(QString {});
        m_stack->setCurrentWidget(m_state);
        return;
    }

    core::stream_filter::ClientFilters filters;
    filters.cachedOnly = m_rdConfigured && m_cachedOnlyCheck->isChecked();
    filters.keywordBlocklist = m_filterSettings.keywordBlocklist();

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
        m_state->showIdle(title, info);
        m_stack->setCurrentWidget(m_state);
        return;
    }
    m_torrents->reset(std::move(visible));
    m_view->sortByColumn(TorrentsModel::ColSeeders, Qt::DescendingOrder);
    m_stack->setCurrentWidget(m_view);
}

void StreamsPanel::rebuildCachedOnlyVisibility()
{
    m_cachedOnlyCheck->setVisible(m_rdConfigured && !m_rawStreams.isEmpty());
}

void StreamsPanel::onTorrentContextMenu(const QPoint& pos)
{
    const auto idx = m_view->indexAt(pos);
    if (!idx.isValid()) {
        return;
    }
    auto* proxy = qobject_cast<QSortFilterProxyModel*>(m_view->model());
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

    // Capture by value so the action still runs if the row changes
    // under us while the menu is up.
    const api::Stream stream = *s;
    if (auto* chosen = menu.exec(m_view->viewport()->mapToGlobal(pos))) {
        if (chosen == play) {
            m_actions.play(stream);
        } else if (chosen == copyMagnet) {
            m_actions.copyMagnet(stream);
        } else if (chosen == openMagnet) {
            m_actions.openMagnet(stream);
        } else if (chosen == copyDirect) {
            m_actions.copyDirectUrl(stream);
        } else if (chosen == openDirect) {
            m_actions.openDirectUrl(stream);
        }
    }
}

} // namespace kinema::ui
