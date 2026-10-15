// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"
#include "ui/qml-bridge/library/LibraryRailModel.h"

#include <QList>
#include <QObject>

namespace kinema::playback::history {
class HistoryQueryService;
}

namespace kinema::ui::qml {

/**
 * View-model behind `ContinueWatchingRail.qml`. Wraps
 * `playback::history::HistoryQueryService` so QML never sees
 * `HistoryEntry` directly — it exposes a `LibraryRailModel`, the
 * same model type the other Up Next rails use, so the rail can
 * render with the shared `EpisodeRailCard` chrome.
 *
 * Re-pulls from the service whenever `HistoryQueryService::changed()`
 * fires (post-record / post-remove); the rail collapses when empty
 * via the `empty` property.
 *
 * Action signals (`resumeRequested`, `detailRequested`,
 * `streamsRequested`, `removeRequested`) are forwarded by
 * `ShellViewModel::wireNavigationRouting`. Each delivers the original
 * `domain::HistoryEntry` so the receiver has the full record (key,
 * stored stream ref, remembered languages) without re-querying the
 * store.
 */
class ContinueWatchingViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(LibraryRailModel* model READ model CONSTANT)
    Q_PROPERTY(bool empty READ empty NOTIFY emptyChanged)

public:
    explicit ContinueWatchingViewModel(playback::history::HistoryQueryService* history,
                                       QObject* parent = nullptr);

    LibraryRailModel* model() const noexcept { return m_model; }
    bool empty() const noexcept { return m_entries.isEmpty(); }

    /// Test/inspector accessor — phase 03 uses this only in the unit
    /// test fixture.
    const QList<domain::HistoryEntry>& entries() const noexcept { return m_entries; }

public Q_SLOTS:
    /// Re-pull entries from the query service and rebuild the rail.
    /// Wired to `HistoryQueryService::changed`; safe to call directly.
    void refresh();

    /// QML hooks: row indices map 1:1 to the rail's display order
    /// (most recent first), which matches `m_entries`.
    void resume(int row);
    void openDetail(int row);
    void openStreams(int row);
    void remove(int row);

Q_SIGNALS:
    void emptyChanged();

    /// Forwarded to `ShellViewModel` which routes them into
    /// `playback::resume::ResumeUseCase::resume`, detail navigation,
    /// direct streams-page navigation, or the history remove path.
    void resumeRequested(const domain::HistoryEntry& entry);
    void detailRequested(const domain::HistoryEntry& entry);
    void streamsRequested(const domain::HistoryEntry& entry);
    void removeRequested(const domain::HistoryEntry& entry);

private:
    void rebuildModel();

    playback::history::HistoryQueryService* m_history;
    LibraryRailModel* m_model;
    QList<domain::HistoryEntry> m_entries;
    bool m_lastEmpty = true;
};

} // namespace kinema::ui::qml
