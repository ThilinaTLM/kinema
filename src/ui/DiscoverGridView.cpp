// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/DiscoverGridView.h"

#include "ui/DiscoverCardDelegate.h"

#include <QAbstractItemModel>
#include <QFrame>
#include <QResizeEvent>
#include <QWheelEvent>

#include <algorithm>

namespace kinema::ui {

namespace {

constexpr int kGridPadding = 8; // per-cell margin baked into grid size

int gridCellWidth()
{
    return DiscoverCardDelegate::kCardWidth + kGridPadding;
}

int gridCellHeight()
{
    return DiscoverCardDelegate::kCardHeight + kGridPadding;
}

} // namespace

DiscoverGridView::DiscoverGridView(QWidget* parent)
    : QListView(parent)
{
    setFlow(QListView::LeftToRight);
    setWrapping(true);
    setMovement(QListView::Static);
    setResizeMode(QListView::Adjust);
    setUniformItemSizes(true);
    setSelectionMode(QAbstractItemView::SingleSelection);
    setSelectionBehavior(QAbstractItemView::SelectItems);
    setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    setMouseTracking(true);
    setFrameShape(QFrame::NoFrame);
    setGridSize(QSize(gridCellWidth(), gridCellHeight()));
    setSpacing(0); // spacing is baked into gridSize

    // We compute our own height; disallow vertical growth beyond it.
    setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
}

void DiscoverGridView::setMaxVisibleRows(int rows)
{
    if (rows == m_maxRows) {
        return;
    }
    m_maxRows = rows;
    recomputeHeight();
}

void DiscoverGridView::setModel(QAbstractItemModel* m)
{
    if (auto* prev = model()) {
        disconnect(prev, nullptr, this, nullptr);
    }
    QListView::setModel(m);
    if (m) {
        connect(m, &QAbstractItemModel::modelReset,
            this, &DiscoverGridView::recomputeHeight);
        connect(m, &QAbstractItemModel::rowsInserted,
            this, [this](const QModelIndex&, int, int) { recomputeHeight(); });
        connect(m, &QAbstractItemModel::rowsRemoved,
            this, [this](const QModelIndex&, int, int) { recomputeHeight(); });
        connect(m, &QAbstractItemModel::layoutChanged,
            this, [this] { recomputeHeight(); });
    }
    recomputeHeight();
}

int DiscoverGridView::currentColumnCount() const
{
    // viewport() is always the usable inner area; with NoFrame it
    // equals this->width(). Before first layout it may be 0 — fall
    // back to the widget's own width so we still give a usable value.
    const int w = viewport() ? viewport()->width() : width();
    const int stride = gridCellWidth();
    if (w <= 0 || stride <= 0) {
        return 1;
    }
    return std::max(1, w / stride);
}

int DiscoverGridView::totalRowCount() const
{
    if (!model()) {
        return 0;
    }
    const int count = model()->rowCount();
    if (count <= 0) {
        return 0;
    }
    const int cols = currentColumnCount();
    return (count + cols - 1) / cols;
}

int DiscoverGridView::heightForRows(int rows) const
{
    if (rows <= 0) {
        return 0;
    }
    return rows * gridCellHeight();
}

void DiscoverGridView::recomputeHeight()
{
    const int total = totalRowCount();
    const int shown = (m_maxRows >= 0)
        ? std::min(total, m_maxRows)
        : total;
    const int h = heightForRows(shown);
    // Even when there's nothing to show, keep a minimal height so
    // layout transitions don't flicker to zero and back.
    const int finalH = std::max(h, heightForRows(1));
    setFixedHeight(finalH);
}

QSize DiscoverGridView::sizeHint() const
{
    // Width hint is a single card so our parent layout decides how
    // much to give us; the real width comes from the resize cycle.
    return { gridCellWidth(), std::max(heightForRows(1), heightForRows(
        (m_maxRows >= 0) ? std::min(totalRowCount(), m_maxRows)
                          : totalRowCount())) };
}

QSize DiscoverGridView::minimumSizeHint() const
{
    return { gridCellWidth(), heightForRows(1) };
}

void DiscoverGridView::resizeEvent(QResizeEvent* e)
{
    QListView::resizeEvent(e);
    // Column count may have changed with the new width, so the
    // capped/total row count changes too — re-apply fixed height.
    if (e->size().width() != e->oldSize().width()) {
        recomputeHeight();
    }
}

void DiscoverGridView::wheelEvent(QWheelEvent* e)
{
    // Ignore so the event propagates to the enclosing QScrollArea
    // (DiscoverPage's rows scroll, or the detail pane's left scroll).
    // Without this, QListView's default wheelEvent would accept and
    // silently try to scroll its own (always-off) scrollbars.
    e->ignore();
}

} // namespace kinema::ui
