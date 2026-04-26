// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QStyledItemDelegate>

namespace kinema::ui::widgets {

/**
 * Card-style row painter for `SubtitleResultsModel`. Renders each
 * row as:
 *
 *     [icon]  Release.Name.Bold.Elided                [Download/Use/Re-attach]
 *             [EN] English  ·  srt  ·  24,531 dl  ·  ★ 8.4   [HI] [FPO]
 *
 * The action button is painted by the delegate but actually clicked
 * via the view's `clicked()` signal — `SubtitlesDialog` holds the
 * action-rect logic and turns clicks inside it into a download
 * request. We expose `actionRectFor(index, option)` so the dialog
 * can hit-test cleanly.
 *
 * Selection / hover use `QPalette::Highlight` like every other Kinema
 * delegate.
 */
class SubtitleResultDelegate : public QStyledItemDelegate
{
    Q_OBJECT
public:
    explicit SubtitleResultDelegate(QObject* parent = nullptr);

    QSize sizeHint(const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;
    void paint(QPainter* painter, const QStyleOptionViewItem& option,
        const QModelIndex& index) const override;

    /// Hit-test rect for the action button on a given row. Computed
    /// from the row rect, no model lookup needed.
    QRect actionRectFor(const QStyleOptionViewItem& option,
        const QModelIndex& index) const;

    /// Visible label on the action button for a given row. Used by
    /// the dialog when the action is fired through the keyboard.
    static QString actionLabelFor(const QModelIndex& index);
};

} // namespace kinema::ui::widgets
