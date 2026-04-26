// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/ContinueWatchingViewModel.h"

#include "controllers/HistoryController.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KLocalizedString>

namespace kinema::ui::qml {

namespace {

/// Build a `DiscoverItem` view of a `HistoryEntry`. The rail's
/// delegate reads only title / poster / kind, so non-display fields
/// stay zeroed.
api::DiscoverItem itemFromHistory(const api::HistoryEntry& e)
{
    api::DiscoverItem it;
    it.kind = e.key.kind;
    it.title = e.seriesTitle.isEmpty() ? e.title : e.seriesTitle;
    it.poster = e.poster;
    return it;
}

/// Continue-Watching subtitle line: "1080p — Release.Name", or the
/// release name alone, or the bare quality label as a last resort.
/// Mirrors `ui::ContinueWatchingSection::lastReleaseLine` so visual
/// parity with the legacy widget surface is preserved during the
/// migration.
QString lastReleaseLine(const api::HistoryStreamRef& ref)
{
    if (ref.isEmpty()) {
        return {};
    }
    if (!ref.resolution.isEmpty() && !ref.releaseName.isEmpty()) {
        return ref.resolution + QStringLiteral(" \u2014 ")
            + ref.releaseName;
    }
    if (!ref.releaseName.isEmpty()) {
        return ref.releaseName;
    }
    return ref.qualityLabel;
}

} // namespace

ContinueWatchingViewModel::ContinueWatchingViewModel(
    controllers::HistoryController* history, QObject* parent)
    : QObject(parent)
    , m_history(history)
    , m_model(new DiscoverSectionModel(
          i18nc("@label discover row", "Continue Watching"), this))
{
    if (m_history) {
        connect(m_history,
            &controllers::HistoryController::changed, this,
            &ContinueWatchingViewModel::refresh);
    }
    refresh();
}

void ContinueWatchingViewModel::refresh()
{
    if (!m_history) {
        m_entries.clear();
        rebuildModel();
        return;
    }
    m_entries = m_history->continueWatching();
    rebuildModel();
}

void ContinueWatchingViewModel::rebuildModel()
{
    QList<api::DiscoverItem> items;
    QList<double> progress;
    QStringList releases;
    items.reserve(m_entries.size());
    progress.reserve(m_entries.size());
    releases.reserve(m_entries.size());
    for (const auto& e : m_entries) {
        items.append(itemFromHistory(e));
        progress.append(e.progressFraction());
        releases.append(lastReleaseLine(e.lastStream));
    }
    m_model->setItems(std::move(items));
    m_model->setProgressList(std::move(progress));
    m_model->setLastReleaseList(std::move(releases));

    const bool nowEmpty = m_entries.isEmpty();
    if (nowEmpty != m_lastEmpty) {
        m_lastEmpty = nowEmpty;
        Q_EMIT emptyChanged();
    }
}

void ContinueWatchingViewModel::resume(int row)
{
    if (row < 0 || row >= m_entries.size()) {
        return;
    }
    Q_EMIT resumeRequested(m_entries.at(row));
}

void ContinueWatchingViewModel::openDetail(int row)
{
    if (row < 0 || row >= m_entries.size()) {
        return;
    }
    Q_EMIT detailRequested(m_entries.at(row));
}

void ContinueWatchingViewModel::remove(int row)
{
    if (row < 0 || row >= m_entries.size()) {
        return;
    }
    Q_EMIT removeRequested(m_entries.at(row));
}

} // namespace kinema::ui::qml
