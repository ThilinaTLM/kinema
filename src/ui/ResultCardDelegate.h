// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QSet>
#include <QStyledItemDelegate>
#include <QUrl>

namespace kinema::ui {

class ImageLoader;

/**
 * Delegate that renders a result as a poster card:
 *
 *   +-------------+
 *   |             |
 *   |   poster    |
 *   |             |
 *   +-------------+
 *    Title (Year)
 *
 * Posters are loaded on demand through ImageLoader. A grey placeholder is
 * drawn while a fetch is in flight; the view is repainted when the poster
 * becomes available.
 */
class ResultCardDelegate : public QStyledItemDelegate
{
    Q_OBJECT
public:
    explicit ResultCardDelegate(ImageLoader* loader, QObject* parent = nullptr);

    void paint(QPainter* painter, const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;
    QSize sizeHint(const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;

    /// Drop per-search state (e.g. the "already requested" set).
    void resetRequestTracking();

    static constexpr int kCardWidth = 180;
    static constexpr int kPosterHeight = 260;
    static constexpr int kLabelHeight = 44;
    static constexpr int kCardHeight = kPosterHeight + kLabelHeight + 12;

private:
    ImageLoader* m_loader;
    /// URLs we have already dispatched a fetch for. Prevents re-firing on
    /// every paint for a failed load.
    mutable QSet<QUrl> m_requested;
};

} // namespace kinema::ui
