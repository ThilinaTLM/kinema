// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QSet>
#include <QStyledItemDelegate>
#include <QUrl>

namespace kinema::ui {

class ImageLoader;

/**
 * Compact sibling of ResultCardDelegate for horizontal strips (home
 * Discover page, "More like this" strip on the detail panes).
 *
 * Layout:
 *
 *   +-----------+
 *   |           |
 *   |  poster   |
 *   |           |
 *   +-----------+
 *    Title
 *    2024
 *
 * Smaller than ResultCardDelegate so 6–8 cards fit across the visible
 * strip width. Paints title + year but no description or rating — the
 * strip is a browse affordance, details live in the detail pane.
 *
 * Posters are loaded on demand through ImageLoader; a grey placeholder
 * is drawn while a fetch is in flight; the view is repainted when the
 * poster becomes available.
 */
class DiscoverCardDelegate : public QStyledItemDelegate
{
    Q_OBJECT
public:
    explicit DiscoverCardDelegate(ImageLoader* loader, QObject* parent = nullptr);

    void paint(QPainter* painter, const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;
    QSize sizeHint(const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;

    void resetRequestTracking();

    static constexpr int kCardWidth = 150;
    static constexpr int kPosterHeight = 220;
    static constexpr int kLabelHeight = 40;
    static constexpr int kCardHeight = kPosterHeight + kLabelHeight + 12;

protected:
    /// Draw the selection / hover background appropriate for `option`
    /// into `option.rect`. Shared with ContinueWatchingCardDelegate.
    void paintBackground(QPainter* painter,
        const QStyleOptionViewItem& option) const;
    /// Draw the poster at `posterRect`, fetching through ImageLoader
    /// on miss. Falls back to a stylised placeholder.
    void paintPoster(QPainter* painter,
        const QRect& posterRect, const QUrl& posterUrl,
        const QPalette& palette) const;
    /// Draw the title at `labelRect` in the bold card font.
    void paintTitle(QPainter* painter, const QRect& labelRect,
        const QStyleOptionViewItem& option,
        const QString& title) const;

private:
    ImageLoader* m_loader;
    mutable QSet<QUrl> m_requested;
};

} // namespace kinema::ui
