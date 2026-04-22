// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/ContinueWatchingSection.h"

#include "controllers/HistoryController.h"
#include "ui/ContinueWatchingCardDelegate.h"
#include "ui/DiscoverGridView.h"
#include "ui/DiscoverRowModel.h"

#include <KLocalizedString>

#include <QAction>
#include <QElapsedTimer>
#include <QIcon>
#include <QMenu>

namespace kinema::ui {

namespace {

/// Build a `DiscoverItem` suitable for rendering the card. Only the
/// fields the delegate reads are populated; everything else stays
/// zeroed.
api::DiscoverItem itemFromHistory(const api::HistoryEntry& e)
{
    api::DiscoverItem it;
    it.kind = e.key.kind;
    it.title = e.seriesTitle.isEmpty() ? e.title : e.seriesTitle;
    it.poster = e.poster;
    return it;
}

/// Continue-Watching display line: "1080p \u2014 Release.Name" or the
/// release name alone when no resolution tag is available.
QString lastReleaseLine(const api::HistoryStreamRef& ref)
{
    if (ref.isEmpty()) {
        return {};
    }
    if (!ref.resolution.isEmpty() && !ref.releaseName.isEmpty()) {
        return ref.resolution + QStringLiteral(" \u2014 ") + ref.releaseName;
    }
    if (!ref.releaseName.isEmpty()) {
        return ref.releaseName;
    }
    return ref.qualityLabel;
}

} // namespace

ContinueWatchingSection::ContinueWatchingSection(const QString& title,
    ImageLoader* images,
    controllers::HistoryController* history, QWidget* parent)
    : DiscoverSection(title, images, parent)
    , m_history(history)
{
    // Replace the base delegate with the Continue-Watching one so
    // cards get a progress bar + last-release secondary line.
    auto* cw = new ContinueWatchingCardDelegate(images, view());
    view()->setItemDelegate(cw);

    installInteractions();

    if (m_history) {
        connect(m_history, &controllers::HistoryController::changed,
            this, &ContinueWatchingSection::refresh);
    }
    refresh();
}

void ContinueWatchingSection::installInteractions()
{
    auto* v = view();
    connect(v, &QAbstractItemView::activated,
        this, &ContinueWatchingSection::onCardActivated);
    connect(v, &QAbstractItemView::clicked,
        this, &ContinueWatchingSection::onCardActivated);

    v->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(v, &QWidget::customContextMenuRequested,
        this, &ContinueWatchingSection::onContextMenuRequested);
}

void ContinueWatchingSection::refresh()
{
    if (!m_history) {
        m_entries.clear();
        model()->reset({});
        setVisible(false);
        return;
    }

    m_entries = m_history->continueWatching();

    if (m_entries.isEmpty()) {
        model()->reset({});
        setVisible(false);
        return;
    }

    QList<api::DiscoverItem> items;
    items.reserve(m_entries.size());
    QList<double> progress;
    progress.reserve(m_entries.size());
    QStringList releases;
    releases.reserve(m_entries.size());
    for (const auto& e : m_entries) {
        items.append(itemFromHistory(e));
        progress.append(e.progressFraction());
        releases.append(lastReleaseLine(e.lastStream));
    }
    model()->reset(items);
    model()->setProgressList(progress);
    model()->setLastReleaseList(releases);
    showItems();
    setVisible(true);
}

void ContinueWatchingSection::onCardActivated(const QModelIndex& idx)
{
    if (!idx.isValid() || idx.row() < 0 || idx.row() >= m_entries.size()) {
        return;
    }
    // Qt fires both `clicked` and `activated` for a single click under
    // KDE's single-click-activation setup. Suppress the duplicate so
    // the resume pipeline (network fetch + KNotification) only runs
    // once per real user interaction.
    if (m_lastActivationRow == idx.row()
        && m_lastActivation.isValid()
        && m_lastActivation.elapsed() < 300) {
        return;
    }
    m_lastActivationRow = idx.row();
    m_lastActivation.restart();
    Q_EMIT resumeRequested(m_entries.at(idx.row()));
}

void ContinueWatchingSection::onContextMenuRequested(const QPoint& pos)
{
    auto* v = view();
    const auto idx = v->indexAt(pos);
    if (!idx.isValid() || idx.row() < 0 || idx.row() >= m_entries.size()) {
        return;
    }
    const auto entry = m_entries.at(idx.row());

    QMenu menu(v);
    auto* resume = menu.addAction(
        QIcon::fromTheme(QStringLiteral("media-playback-start")),
        i18nc("@action:inmenu", "&Resume"));
    menu.setDefaultAction(resume);
    auto* choose = menu.addAction(
        QIcon::fromTheme(QStringLiteral("view-list-details")),
        i18nc("@action:inmenu", "Choose another release\u2026"));
    menu.addSeparator();
    auto* remove = menu.addAction(
        QIcon::fromTheme(QStringLiteral("edit-delete")),
        i18nc("@action:inmenu", "Remove from history"));

    if (auto* chosen = menu.exec(v->viewport()->mapToGlobal(pos))) {
        if (chosen == resume) {
            Q_EMIT resumeRequested(entry);
        } else if (chosen == choose) {
            Q_EMIT detailRequested(entry);
        } else if (chosen == remove) {
            Q_EMIT removeRequested(entry);
        }
    }
}

} // namespace kinema::ui
