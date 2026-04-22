// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"
#include "ui/DiscoverSection.h"

#include <QElapsedTimer>
#include <QList>

namespace kinema::controllers {
class HistoryController;
}

namespace kinema::ui {

class ImageLoader;

/**
 * Discover-row variant fed by `controllers::HistoryController`, not
 * TMDB. Shows the user's most-recent in-progress items with a
 * progress bar + "saved release" line on each card. Hidden entirely
 * when the history is empty.
 *
 * Activation signals are richer than the base section because
 * MainWindow needs to route plain-click (resume), context-menu
 * "Choose another release…" (open detail pane), and context-menu
 * "Remove from history" (delete row) to different code paths.
 */
class ContinueWatchingSection : public DiscoverSection
{
    Q_OBJECT
public:
    ContinueWatchingSection(const QString& title,
        ImageLoader* images,
        controllers::HistoryController* history,
        QWidget* parent = nullptr);

    /// Rebuild the section from the history controller. No-op when
    /// the controller is null.
    void refresh();

    /// The entries currently on display, ordered the same as the
    /// model rows. Used by MainWindow to resolve an activated index
    /// back into a HistoryEntry.
    const QList<api::HistoryEntry>& entries() const noexcept
    {
        return m_entries;
    }

Q_SIGNALS:
    /// Plain click / Enter on a card. MainWindow forwards to
    /// HistoryController::resumeFromHistory.
    void resumeRequested(const api::HistoryEntry& entry);
    /// Context-menu "Choose another release…". MainWindow opens the
    /// detail pane via openFromHistory.
    void detailRequested(const api::HistoryEntry& entry);
    /// Context-menu "Remove from history".
    void removeRequested(const api::HistoryEntry& entry);

private Q_SLOTS:
    void onCardActivated(const QModelIndex& idx);
    void onContextMenuRequested(const QPoint& pos);

private:
    void installInteractions();

    controllers::HistoryController* m_history;
    QList<api::HistoryEntry> m_entries;

    /// Debounce state for the single-click / activated double-fire.
    /// Qt emits both `clicked` and `activated` on KDE's
    /// single-click activation and we want one resume per real click.
    QElapsedTimer m_lastActivation;
    int m_lastActivationRow = -1;
};

} // namespace kinema::ui
