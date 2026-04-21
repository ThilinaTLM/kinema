// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/DiscoverCardDelegate.h"

#include "ui/DiscoverRowModel.h"
#include "ui/ImageLoader.h"

#include <QAbstractItemView>
#include <QPainter>
#include <QPalette>
#include <QPixmap>

namespace kinema::ui {

DiscoverCardDelegate::DiscoverCardDelegate(ImageLoader* loader, QObject* parent)
    : QStyledItemDelegate(parent)
    , m_loader(loader)
{
    if (m_loader) {
        QObject::connect(m_loader, &ImageLoader::posterReady, this,
            [this](const QUrl& url) {
                // Release the in-flight marker so a later cache
                // eviction can re-request the poster. ImageLoader's
                // m_inFlight already de-dupes concurrent fetches, so
                // re-issuing after eviction is free (disk-cache hit).
                m_requested.remove(url);
                auto* view = qobject_cast<QAbstractItemView*>(this->parent());
                if (view && view->viewport()) {
                    view->viewport()->update();
                }
            });
    }
}

void DiscoverCardDelegate::resetRequestTracking()
{
    m_requested.clear();
}

QSize DiscoverCardDelegate::sizeHint(const QStyleOptionViewItem&, const QModelIndex&) const
{
    return { kCardWidth, kCardHeight };
}

void DiscoverCardDelegate::paint(QPainter* painter, const QStyleOptionViewItem& option,
    const QModelIndex& index) const
{
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing, true);
    painter->setRenderHint(QPainter::SmoothPixmapTransform, true);

    const auto palette = option.palette;
    if (option.state & QStyle::State_Selected) {
        painter->fillRect(option.rect, palette.highlight());
    } else if (option.state & QStyle::State_MouseOver) {
        auto hover = palette.color(QPalette::Highlight);
        hover.setAlpha(40);
        painter->fillRect(option.rect, hover);
    }

    const QRect r = option.rect.adjusted(4, 6, -4, -6);
    const QRect posterRect(r.left(), r.top(), r.width(), kPosterHeight);

    const auto posterUrl = index.data(DiscoverRowModel::PosterUrlRole).toUrl();
    QPixmap pm;
    if (!posterUrl.isEmpty() && m_loader) {
        pm = m_loader->cached(posterUrl);
        if (pm.isNull() && !m_requested.contains(posterUrl)) {
            m_requested.insert(posterUrl);
            m_loader->requestPoster(posterUrl);
        }
    }

    if (!pm.isNull()) {
        const auto scaled = pm.scaled(posterRect.size(),
            Qt::KeepAspectRatio, Qt::SmoothTransformation);
        const QRect targetRect(
            posterRect.left() + (posterRect.width() - scaled.width()) / 2,
            posterRect.top() + (posterRect.height() - scaled.height()) / 2,
            scaled.width(), scaled.height());
        painter->drawPixmap(targetRect, scaled);
    } else {
        painter->save();
        painter->setPen(Qt::NoPen);
        painter->setBrush(palette.color(QPalette::AlternateBase));
        painter->drawRoundedRect(posterRect, 6, 6);
        auto placeholder = palette.color(QPalette::Text);
        placeholder.setAlphaF(0.55f);
        painter->setPen(placeholder);
        auto f = painter->font();
        f.setPointSizeF(f.pointSizeF() * 2.0);
        painter->setFont(f);
        painter->drawText(posterRect, Qt::AlignCenter, QStringLiteral("🎬"));
        painter->restore();
    }

    // Title label.
    const QRect labelRect(r.left(), r.top() + kPosterHeight + 6, r.width(), kLabelHeight);
    auto font = option.font;
    font.setBold(true);
    painter->setFont(font);
    painter->setPen((option.state & QStyle::State_Selected)
            ? palette.color(QPalette::HighlightedText)
            : palette.color(QPalette::Text));
    const auto title = index.data(DiscoverRowModel::TitleRole).toString();
    painter->drawText(labelRect,
        Qt::AlignHCenter | Qt::AlignTop | Qt::TextWordWrap, title);

    painter->restore();
}

} // namespace kinema::ui
