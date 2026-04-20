// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/ResultCardDelegate.h"

#include "ui/ImageLoader.h"
#include "ui/ResultsModel.h"

#include <QAbstractItemView>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QPixmapCache>

namespace kinema::ui {

namespace {
QString pixmapCacheKey(const QUrl& url)
{
    return QStringLiteral("kinema:poster:") + url.toString(QUrl::FullyEncoded);
}
} // namespace

ResultCardDelegate::ResultCardDelegate(ImageLoader* loader, QObject* parent)
    : QStyledItemDelegate(parent)
    , m_loader(loader)
{
    if (m_loader) {
        // Repaint the owning view when any poster arrives. We don't know
        // which row(s) the URL belongs to, so we just repaint the whole
        // viewport — cheap, and only triggered on genuinely-new posters.
        QObject::connect(m_loader, &ImageLoader::posterReady, this,
            [this](const QUrl&) {
                auto* view = qobject_cast<QAbstractItemView*>(this->parent());
                if (view && view->viewport()) {
                    view->viewport()->update();
                }
            });
    }
}

void ResultCardDelegate::resetRequestTracking()
{
    m_requested.clear();
}

QSize ResultCardDelegate::sizeHint(const QStyleOptionViewItem&, const QModelIndex&) const
{
    return { kCardWidth, kCardHeight };
}

void ResultCardDelegate::paint(QPainter* painter, const QStyleOptionViewItem& option,
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

    const QRect r = option.rect.adjusted(6, 6, -6, -6);
    const QRect posterRect(r.left(), r.top(), r.width(), kPosterHeight);

    const auto posterUrl = index.data(ResultsModel::PosterUrlRole).toUrl();
    QPixmap pm;
    if (!posterUrl.isEmpty()) {
        QPixmapCache::find(pixmapCacheKey(posterUrl), &pm);
        if (pm.isNull() && !m_requested.contains(posterUrl) && m_loader) {
            m_requested.insert(posterUrl);
            // Fire-and-forget: posterReady signal will repaint when done.
            // The coroutine runs to completion on the event loop; we don't
            // need to hold the returned task.
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
        painter->setPen(palette.color(QPalette::Mid));
        auto f = painter->font();
        f.setPointSizeF(f.pointSizeF() * 2.5);
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
    const auto title = index.data(ResultsModel::TitleRole).toString();
    painter->drawText(labelRect, Qt::AlignHCenter | Qt::AlignTop | Qt::TextWordWrap, title);

    painter->restore();
}

} // namespace kinema::ui
