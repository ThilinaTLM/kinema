// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QWidget>

class QLabel;
class QStackedWidget;
class QToolButton;
class QModelIndex;

namespace kinema::ui {

class DiscoverCardDelegate;
class DiscoverGridView;
class DiscoverRowModel;
class ImageLoader;
class StateWidget;

/**
 * One titled section of the Discover page (or the "More like this"
 * strip on a detail pane). Bundles:
 *
 *   header: [ bold title ............................ Show more ▾ ]
 *   body:   [ StackedWidget { StateWidget | DiscoverGridView }     ]
 *
 * Responsibilities:
 *   - Owns the row model, grid view, card delegate, and state widget.
 *   - Routes state transitions (loading / error / empty / items)
 *     through a single API.
 *   - Manages collapsed (N-rows) vs expanded (all items) modes with
 *     a "Show more" / "Show less" toggle. The toggle hides itself
 *     when expanding wouldn't reveal anything new (e.g. the model
 *     already fits in the collapsed height).
 *   - Emits activation and retry signals for the parent to route.
 *
 * Intentionally oblivious to TMDB or the fetch pipeline; the parent
 * widget (DiscoverPage or SimilarStrip) orchestrates that.
 */
class DiscoverSection : public QWidget
{
    Q_OBJECT
public:
    DiscoverSection(const QString& title,
        ImageLoader* images,
        QWidget* parent = nullptr);

    DiscoverRowModel* model() const noexcept { return m_model; }
    DiscoverGridView* view() const noexcept { return m_view; }
    StateWidget* state() const noexcept { return m_state; }
    DiscoverCardDelegate* delegate() const noexcept { return m_delegate; }

    /// Swap the stack to show state (loading / empty / error) or to
    /// show the grid. showItems() also re-applies the collapsed state
    /// and refreshes the toggle's visibility.
    void showLoading();
    void showEmpty(const QString& body);
    void showError(const QString& message, bool retryable);
    void showItems();

    /// Forces the collapsed/expanded state; keeps the toggle in sync.
    void setCollapsed(bool collapsed);
    bool isCollapsed() const noexcept { return m_collapsed; }

    /// Number of rows shown when collapsed. Default 2.
    void setCollapsedRows(int rows);
    int collapsedRows() const noexcept { return m_collapsedRows; }

Q_SIGNALS:
    /// The user clicked / Enter-pressed a card at `idx` (in the
    /// DiscoverRowModel). Parent resolves the item and routes it.
    void itemActivated(const QModelIndex& idx);

    /// The state widget's retry button was pressed.
    void retryRequested();

protected:
    void resizeEvent(QResizeEvent* e) override;

private:
    void applyCollapsedState();
    void updateToggleVisibility();

    QLabel* m_title {};
    QToolButton* m_toggle {};
    QStackedWidget* m_stack {};
    StateWidget* m_state {};
    DiscoverGridView* m_view {};
    DiscoverRowModel* m_model {};
    DiscoverCardDelegate* m_delegate {};

    int m_collapsedRows = 2;
    bool m_collapsed = true;
    bool m_showingItems = false; // stack currently on the grid view
};

} // namespace kinema::ui
