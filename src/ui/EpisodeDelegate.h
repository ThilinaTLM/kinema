// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QSet>
#include <QStyledItemDelegate>
#include <QUrl>

namespace kinema::ui {

class ImageLoader;

/**
 * Delegate that renders a single episode row:
 *
 *   +--------+  1x01 — Pilot
 *   | thumb  |  Jan 20, 2008
 *   +--------+  (description excerpt)
 *
 * Thumbnails come from ImageLoader — same two-level cache as posters.
 */
class EpisodeDelegate : public QStyledItemDelegate
{
    Q_OBJECT
public:
    explicit EpisodeDelegate(ImageLoader* loader, QObject* parent = nullptr);

    void paint(QPainter* painter, const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;
    QSize sizeHint(const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;

    void resetRequestTracking();

    static constexpr int kThumbWidth = 128;
    static constexpr int kThumbHeight = 72;
    static constexpr int kRowPadding = 8;
    static constexpr int kRowHeight = kThumbHeight + 2 * kRowPadding;

private:
    ImageLoader* m_loader;
    mutable QSet<QUrl> m_requested;
};

} // namespace kinema::ui
