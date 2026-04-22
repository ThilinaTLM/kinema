// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "ui/DiscoverCardDelegate.h"

namespace kinema::ui {

/**
 * DiscoverCardDelegate variant that renders Continue-Watching cards:
 *
 *   - Thin progress bar pinned at the bottom of the poster, painted
 *     only when DiscoverRowModel::ProgressRole > 0.
 *   - Secondary line below the title showing the saved release
 *     (DiscoverRowModel::LastReleaseRole), elided and dimmed. Rows
 *     that leave LastReleaseRole empty fall back to the same layout
 *     as the base delegate.
 *
 * The extra line adds a fixed amount of vertical space so every card
 * in a Continue-Watching grid stays on a uniform-size grid
 * (QListView::setUniformItemSizes relies on this).
 */
class ContinueWatchingCardDelegate : public DiscoverCardDelegate
{
    Q_OBJECT
public:
    explicit ContinueWatchingCardDelegate(ImageLoader* loader,
        QObject* parent = nullptr);

    void paint(QPainter* painter, const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;
    QSize sizeHint(const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;

    /// Height of the secondary "last release" line.
    static constexpr int kSecondaryLineHeight = 16;
    /// Progress-bar height (rounded rect).
    static constexpr int kProgressBarHeight = 3;
};

} // namespace kinema::ui
