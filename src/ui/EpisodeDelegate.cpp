// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/EpisodeDelegate.h"

#include "ui/EpisodesModel.h"
#include "ui/ImageLoader.h"

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

EpisodeDelegate::EpisodeDelegate(ImageLoader* loader, QObject* parent)
    : QStyledItemDelegate(parent)
    , m_loader(loader)
{
    if (m_loader) {
        QObject::connect(m_loader, &ImageLoader::posterReady, this,
            [this](const QUrl&) {
                auto* view = qobject_cast<QAbstractItemView*>(this->parent());
                if (view && view->viewport()) {
                    view->viewport()->update();
                }
            });
    }
}

void EpisodeDelegate::resetRequestTracking()
{
    m_requested.clear();
}

QSize EpisodeDelegate::sizeHint(const QStyleOptionViewItem& option, const QModelIndex&) const
{
    return { option.rect.width(), kRowHeight };
}

void EpisodeDelegate::paint(QPainter* painter, const QStyleOptionViewItem& option,
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

    const QRect r = option.rect.adjusted(kRowPadding, kRowPadding, -kRowPadding, -kRowPadding);
    const QRect thumbRect(r.left(), r.top(), kThumbWidth, kThumbHeight);

    // Thumbnail.
    const auto thumbUrl = index.data(EpisodesModel::ThumbnailUrlRole).toUrl();
    QPixmap pm;
    if (!thumbUrl.isEmpty()) {
        QPixmapCache::find(pixmapCacheKey(thumbUrl), &pm);
        if (pm.isNull() && !m_requested.contains(thumbUrl) && m_loader) {
            m_requested.insert(thumbUrl);
            m_loader->requestPoster(thumbUrl);
        }
    }

    if (!pm.isNull()) {
        const auto scaled = pm.scaled(thumbRect.size(),
            Qt::KeepAspectRatio, Qt::SmoothTransformation);
        const QRect targetRect(
            thumbRect.left() + (thumbRect.width() - scaled.width()) / 2,
            thumbRect.top() + (thumbRect.height() - scaled.height()) / 2,
            scaled.width(), scaled.height());
        painter->drawPixmap(targetRect, scaled);
    } else {
        painter->save();
        painter->setPen(Qt::NoPen);
        painter->setBrush(palette.color(QPalette::AlternateBase));
        painter->drawRoundedRect(thumbRect, 4, 4);
        painter->restore();
    }

    // Text block, right of the thumbnail.
    const int textLeft = thumbRect.right() + 12;
    const QRect textRect(textLeft, r.top(), r.right() - textLeft, r.height());

    const int number = index.data(EpisodesModel::NumberRole).toInt();
    const auto title = index.data(EpisodesModel::TitleRole).toString();
    const auto released = index.data(EpisodesModel::ReleasedRole).toString();
    const auto description = index.data(EpisodesModel::DescriptionRole).toString();

    auto titleFont = option.font;
    titleFont.setBold(true);
    painter->setFont(titleFont);
    painter->setPen((option.state & QStyle::State_Selected)
            ? palette.color(QPalette::HighlightedText)
            : palette.color(QPalette::Text));

    const QString headline = number > 0
        ? QStringLiteral("%1 — %2").arg(number).arg(title)
        : title;
    const QFontMetrics tfm(titleFont);
    const int titleH = tfm.lineSpacing();
    painter->drawText(QRect(textRect.left(), textRect.top(), textRect.width(), titleH),
        Qt::AlignLeft | Qt::AlignVCenter,
        tfm.elidedText(headline, Qt::ElideRight, textRect.width()));

    // Release date + description lines. Use a dimmed variant of the
    // primary text colour for visual hierarchy. QPalette::Mid would be
    // wrong here — it's a border/shadow role, not a text role, and on
    // dark themes resolves to a near-black that's invisible on the
    // list's dark Base background.
    auto subFont = option.font;
    subFont.setPointSizeF(subFont.pointSizeF() * 0.9);
    painter->setFont(subFont);
    auto secondary = (option.state & QStyle::State_Selected)
        ? palette.color(QPalette::HighlightedText)
        : palette.color(QPalette::Text);
    secondary.setAlphaF(0.65f);
    painter->setPen(secondary);
    const QFontMetrics sfm(subFont);
    if (!released.isEmpty()) {
        painter->drawText(QRect(textRect.left(), textRect.top() + titleH + 2,
                             textRect.width(), sfm.lineSpacing()),
            Qt::AlignLeft | Qt::AlignVCenter, released);
    }

    // Description excerpt (third line).
    if (!description.isEmpty()) {
        const int descY = textRect.top() + titleH + sfm.lineSpacing() + 6;
        if (descY + sfm.lineSpacing() <= textRect.bottom()) {
            painter->drawText(QRect(textRect.left(), descY, textRect.width(), sfm.lineSpacing()),
                Qt::AlignLeft | Qt::AlignVCenter,
                sfm.elidedText(description, Qt::ElideRight, textRect.width()));
        }
    }

    painter->restore();
}

} // namespace kinema::ui
