// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/ContinueWatchingViewModel.h"

#include "controllers/HistoryController.h"
#include "ui/qml-bridge/LibraryRailModel.h"

#include <KLocalizedString>

#include <QtGlobal>

namespace kinema::ui::qml {

namespace {

/// Episode designator "S01E02" (zero-padded) for a series row.
/// Empty for movies or rows missing season/episode.
QString episodeCode(const domain::HistoryEntry& e)
{
    if (e.key.kind != domain::MediaKind::Series
        || !e.key.season.has_value() || !e.key.episode.has_value()) {
        return {};
    }
    return QStringLiteral("S%1E%2")
        .arg(*e.key.season, 2, 10, QLatin1Char('0'))
        .arg(*e.key.episode, 2, 10, QLatin1Char('0'));
}

/// Build the secondary text line for an `EpisodeRailCard` row.
/// Series rows render `S01E02 · Episode title` (or just `S01E02`
/// when the episode title is empty). Movie rows have no secondary
/// line so the meta block collapses to two lines, matching how
/// Ready to Watch handles movies.
QString secondaryLineFor(const domain::HistoryEntry& e)
{
    const auto code = episodeCode(e);
    if (code.isEmpty()) {
        return {};
    }
    if (e.episodeTitle.isEmpty()) {
        return code;
    }
    return i18nc("@info library rail, episode code dot title",
        "%1 \u00b7 %2", code, e.episodeTitle);
}

/// Build the tertiary text line ("Resume from 42%") when the entry
/// has positive playback progress. Empty otherwise — the card hides
/// empty meta lines.
QString tertiaryLineFor(double progressFraction)
{
    if (!(progressFraction > 0.0)) {
        return {};
    }
    const int percent = qRound(
        qBound(0.0, progressFraction, 1.0) * 100.0);
    return i18nc("@info continue-watching, resume percent",
        "Resume from %1%", percent);
}

LibraryRailRow rowFromHistory(const domain::HistoryEntry& e)
{
    LibraryRailRow row;
    row.kind = e.key.kind;
    row.imdbId = e.key.imdbId;
    row.season = e.key.season;
    row.episode = e.key.episode;
    row.title = e.seriesTitle.isEmpty() ? e.title : e.seriesTitle;
    row.posterUrl = e.poster.toString();
    // `backdrop_url` is captured at play time (see
    // `HistoryController::onPlayStarting`). When present, the
    // EpisodeRailCard renders the 16:9 backdrop natively. Pre-v9
    // history rows / external plays without a backdrop fall through
    // to the letterboxed-poster path. `thumbnailUrl` stays empty
    // because Continue Watching points at a parent title (the same
    // surface the user resumed), not a specific episode still.
    row.backdropUrl = e.backdrop.toString();
    row.primaryLine = row.title;
    row.secondaryLine = secondaryLineFor(e);
    row.progress = qBound(0.0, e.progressFraction(), 1.0);
    row.tertiaryLine = tertiaryLineFor(e.progressFraction());
    return row;
}

} // namespace

ContinueWatchingViewModel::ContinueWatchingViewModel(
    controllers::HistoryController* history, QObject* parent)
    : QObject(parent)
    , m_history(history)
    , m_model(new LibraryRailModel(this))
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
    QList<LibraryRailRow> rows;
    rows.reserve(m_entries.size());
    for (const auto& e : m_entries) {
        rows.append(rowFromHistory(e));
    }
    m_model->setRows(std::move(rows));

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

void ContinueWatchingViewModel::openStreams(int row)
{
    if (row < 0 || row >= m_entries.size()) {
        return;
    }
    Q_EMIT streamsRequested(m_entries.at(row));
}

void ContinueWatchingViewModel::remove(int row)
{
    if (row < 0 || row >= m_entries.size()) {
        return;
    }
    Q_EMIT removeRequested(m_entries.at(row));
}

} // namespace kinema::ui::qml
