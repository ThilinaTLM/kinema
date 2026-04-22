// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/ContinueWatchingCardDelegate.h"

#include "ui/DiscoverRowModel.h"

#include <QFontMetrics>
#include <QPainter>
#include <QPalette>

namespace kinema::ui {

ContinueWatchingCardDelegate::ContinueWatchingCardDelegate(
    ImageLoader* loader, QObject* parent)
    : DiscoverCardDelegate(loader, parent)
{
}

QSize ContinueWatchingCardDelegate::sizeHint(
    const QStyleOptionViewItem&, const QModelIndex&) const
{
    return { kCardWidth, kCardHeight + kSecondaryLineHeight };
}

void ContinueWatchingCardDelegate::paint(QPainter* painter,
    const QStyleOptionViewItem& option, const QModelIndex& index) const
{
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing, true);
    painter->setRenderHint(QPainter::SmoothPixmapTransform, true);

    paintBackground(painter, option);

    const QRect r = option.rect.adjusted(4, 6, -4, -6);
    const QRect posterRect(r.left(), r.top(), r.width(), kPosterHeight);
    paintPoster(painter, posterRect,
        index.data(DiscoverRowModel::PosterUrlRole).toUrl(),
        option.palette);

    // Progress bar overlay pinned to the bottom of the poster.
    const double progress = index.data(DiscoverRowModel::ProgressRole)
                                .toDouble();
    if (progress > 0.0) {
        const int margin = 4;
        const QRect barOuter(posterRect.left() + margin,
            posterRect.bottom() - kProgressBarHeight - margin,
            posterRect.width() - 2 * margin,
            kProgressBarHeight);
        painter->save();
        painter->setPen(Qt::NoPen);
        auto trackColor = option.palette.color(QPalette::Window);
        trackColor.setAlpha(180);
        painter->setBrush(trackColor);
        painter->drawRoundedRect(barOuter, 1.5, 1.5);
        const int fillW = static_cast<int>(
            qBound(0.0, progress, 1.0) * barOuter.width());
        if (fillW > 0) {
            const QRect fillRect(barOuter.left(), barOuter.top(),
                fillW, barOuter.height());
            painter->setBrush(option.palette.color(QPalette::Highlight));
            painter->drawRoundedRect(fillRect, 1.5, 1.5);
        }
        painter->restore();\
    }\

    const QRect labelRect(r.left(), r.top() + kPosterHeight + 6,
        r.width(), kLabelHeight);
    paintTitle(painter, labelRect, option,
        index.data(DiscoverRowModel::TitleRole).toString());

    // Secondary "last release" line below the title.
    const auto release = index.data(DiscoverRowModel::LastReleaseRole)
                             .toString();
    if (!release.isEmpty()) {
        const QRect subRect(r.left(), labelRect.bottom() + 2,
            r.width(), kSecondaryLineHeight);
        painter->save();
        auto sub = option.palette.color(QPalette::PlaceholderText);
        if (option.state & QStyle::State_Selected) {
            sub = option.palette.color(QPalette::HighlightedText);
            sub.setAlphaF(0.75f);
        }
        painter->setPen(sub);
        auto f = painter->font();
        f.setBold(false);
        if (f.pointSizeF() > 0.0) {
            f.setPointSizeF(f.pointSizeF() * 0.85);
        }
        painter->setFont(f);
        const QFontMetrics fm(f);
        const auto elided = fm.elidedText(release, Qt::ElideRight,
            subRect.width());
        painter->drawText(subRect,
            Qt::AlignHCenter | Qt::AlignTop, elided);
        painter->restore();
    }

    painter->restore();
}

} // namespace kinema::ui
