// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QListView>

class QResizeEvent;
class QWheelEvent;

namespace kinema::ui {

/**
 * QListView specialised for the Discover / "More like this" grid:
 *
 *   - Items wrap across as many columns as fit in the viewport.
 *   - The view reports back a height that fits every visible row, so
 *     the outer QScrollArea scrolls through everything and this view
 *     never shows its own scrollbar.
 *   - Callers can cap the visible rows with setMaxVisibleRows() for
 *     the collapsed ("Show more" hidden) mode. Items past the cap are
 *     still in the model; they are just clipped from view.
 *
 * Card geometry comes from DiscoverCardDelegate (kCardWidth /
 * kCardHeight); the delegate's sizeHint is respected via setGridSize.
 */
class DiscoverGridView : public QListView
{
    Q_OBJECT
public:
    explicit DiscoverGridView(QWidget* parent = nullptr);

    /// Cap visible rows. -1 = no cap (show all items). Defaults to 2.
    void setMaxVisibleRows(int rows);
    int maxVisibleRows() const noexcept { return m_maxRows; }

    /// Columns the current viewport width can fit. Always >= 1.
    int currentColumnCount() const;
    /// Total rows required to display every item at the current
    /// column count, ignoring the cap. 0 when the model is empty.
    int totalRowCount() const;

    QSize sizeHint() const override;
    QSize minimumSizeHint() const override;

    void setModel(QAbstractItemModel* model) override;

protected:
    void resizeEvent(QResizeEvent* e) override;
    /// The grid never scrolls internally — let wheel events bubble
    /// up to the enclosing QScrollArea.
    void wheelEvent(QWheelEvent* e) override;

private:
    /// Pushes the computed fixed height onto the view based on the
    /// current viewport width, row count, and m_maxRows cap.
    void recomputeHeight();
    int heightForRows(int rows) const;

    int m_maxRows = 2;
};

} // namespace kinema::ui
