// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <QList>
#include <QObject>

namespace kinema::controllers {
class HistoryController;
}

namespace kinema::ui::qml {

/**
 * View-model behind `ContinueWatchingRail.qml`. Wraps
 * `controllers::HistoryController` so QML never sees `HistoryEntry`
 * directly — it walks a `DiscoverSectionModel` row by row instead,
 * consistent with the other Discover rails.
 *
 * Re-pulls from the controller whenever `HistoryController::changed()`
 * fires (post-record / post-remove); the rail collapses when empty
 * via the `empty` property.
 *
 * Action signals (`resumeRequested`, `detailRequested`,
 * `removeRequested`) are forwarded to the controller in
 * `MainController::buildCoreServices`. Each delivers the original
 * `api::HistoryEntry` so the controller has the full record (key,
 * stored stream ref, remembered languages) without re-querying the
 * store.
 */
class ContinueWatchingViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(DiscoverSectionModel* model READ model CONSTANT)
    Q_PROPERTY(bool empty READ empty NOTIFY emptyChanged)

public:
    explicit ContinueWatchingViewModel(
        controllers::HistoryController* history,
        QObject* parent = nullptr);

    DiscoverSectionModel* model() const noexcept { return m_model; }
    bool empty() const noexcept { return m_entries.isEmpty(); }

    /// Test/inspector accessor — phase 03 uses this only in the unit
    /// test fixture.
    const QList<api::HistoryEntry>& entries() const noexcept
    {
        return m_entries;
    }

public Q_SLOTS:
    /// Re-pull entries from the controller and rebuild the rail.
    /// Wired to `HistoryController::changed`; safe to call directly.
    void refresh();

    /// QML hooks: row indices map 1:1 to the rail's display order
    /// (most recent first), which matches `m_entries`.
    void resume(int row);
    void openDetail(int row);
    void remove(int row);

Q_SIGNALS:
    void emptyChanged();

    /// Forwarded to `MainController` which routes them back into
    /// `HistoryController::resumeFromHistory` / detail navigation /
    /// `HistoryController::removeEntry`.
    void resumeRequested(const api::HistoryEntry& entry);
    void detailRequested(const api::HistoryEntry& entry);
    void removeRequested(const api::HistoryEntry& entry);

private:
    void rebuildModel();

    controllers::HistoryController* m_history;
    DiscoverSectionModel* m_model;
    QList<api::HistoryEntry> m_entries;
    bool m_lastEmpty = true;
};

} // namespace kinema::ui::qml
