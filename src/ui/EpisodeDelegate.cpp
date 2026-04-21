// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/EpisodeDelegate.h"

#include "ui/EpisodesModel.h"
#include "ui/ImageLoader.h"
#include "ui/UpcomingBadge.h"

#include <KLocalizedString>

#include <QAbstractItemView>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QTextLayout>
#include <QTextOption>

namespace kinema::ui {

EpisodeDelegate::EpisodeDelegate(ImageLoader* loader, QObject* parent)
    : QStyledItemDelegate(parent)
    , m_loader(loader)
{
    if (m_loader) {
        QObject::connect(m_loader, &ImageLoader::posterReady, this,
            [this](const QUrl& url) {
                // Release the in-flight marker so a later cache
                // eviction can re-request the thumbnail. ImageLoader's
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

void EpisodeDelegate::resetRequestTracking()
{
    m_requested.clear();
}

QSize EpisodeDelegate::sizeHint(const QStyleOptionViewItem& option, const QModelIndex&) const
{
    // Only the row height matters here. Returning `option.rect.width()`
    // combined with QListView::setUniformItemSizes(true) creates a
    // feedback loop: the first sizeHint is cached at the initial
    // viewport width, then when the vertical scrollbar appears the
    // viewport shrinks but items stay at the wider cached width,
    // producing a spurious horizontal scrollbar. A zero width lets the
    // view fall back to viewport width, which is what we actually
    // want — each row fills the list horizontally.
    Q_UNUSED(option);
    return { 0, kRowHeight };
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
    if (!thumbUrl.isEmpty() && m_loader) {
        pm = m_loader->cached(thumbUrl);
        if (pm.isNull() && !m_requested.contains(thumbUrl)) {
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
    const bool isFuture = index.data(EpisodesModel::FutureReleaseRole).toBool();
    const auto fullEpisode = index.data(EpisodesModel::EpisodeRole)
                                 .value<api::Episode>();

    auto titleFont = option.font;
    titleFont.setBold(true);
    painter->setFont(titleFont);
    painter->setPen((option.state & QStyle::State_Selected)
            ? palette.color(QPalette::HighlightedText)
            : palette.color(QPalette::Text));

    // Fall back to a localized "Episode N" when Cinemeta didn't give
    // us a name (happens for a handful of shows). Better than a bare
    // "1 — " with nothing after the em-dash.
    const QString displayName = title.isEmpty()
        ? i18nc("@label fallback when episode has no title",
              "Episode %1", number)
        : title;
    const QString headline = number > 0
        ? QStringLiteral("%1 — %2").arg(number).arg(displayName)
        : displayName;
    const QFontMetrics tfm(titleFont);
    const int titleH = tfm.lineSpacing();

    // Reserve room for the "Upcoming" badge on the title row so the
    // title elides around it rather than under it.
    int titleWidth = textRect.width();
    QRect badgeRect;
    if (isFuture && fullEpisode.released.has_value()) {
        const QSize bs = upcomingBadgeSize(*fullEpisode.released, option.font);
        badgeRect = QRect(textRect.right() - bs.width(),
            textRect.top() + (titleH - bs.height()) / 2,
            bs.width(), bs.height());
        titleWidth = std::max(0, textRect.width() - bs.width() - 8);
    }
    painter->drawText(QRect(textRect.left(), textRect.top(), titleWidth, titleH),
        Qt::AlignLeft | Qt::AlignVCenter,
        tfm.elidedText(headline, Qt::ElideRight, titleWidth));
    if (isFuture && fullEpisode.released.has_value()) {
        paintUpcomingBadge(painter, badgeRect, *fullEpisode.released,
            palette, option.font);
    }

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

    // Description excerpt — up to two wrapped lines, eliding the last
    // line if the text overflows. Fixed row height means we can't
    // word-wrap indefinitely; two lines fit comfortably in the text
    // column alongside the 72 px thumbnail.
    if (!description.isEmpty()) {
        const int descY = textRect.top() + titleH + sfm.lineSpacing() + 6;
        const int descMaxH = textRect.bottom() - descY;
        const int lineH = sfm.lineSpacing();
        const int maxLines = std::max(1, std::min(2, descMaxH / lineH));

        QTextOption opt;
        opt.setWrapMode(QTextOption::WrapAtWordBoundaryOrAnywhere);

        QTextLayout layout(description, subFont);
        layout.setTextOption(opt);
        layout.beginLayout();

        QList<QTextLine> lines;
        while (lines.size() < maxLines) {
            QTextLine line = layout.createLine();
            if (!line.isValid()) {
                break;
            }
            line.setLineWidth(textRect.width());
            lines.append(line);
        }
        // Probe one more line to detect overflow: if createLine() still
        // returns a valid line, there's more text than we kept.
        bool overflowedBeyondMax = false;
        if (lines.size() == maxLines) {
            QTextLine extra = layout.createLine();
            if (extra.isValid()) {
                overflowedBeyondMax = true;
            }
        }
        layout.endLayout();

        // Detect overflow by either the probe above or a short-read
        // from createLine() that hit a natural end before maxLines.
        int consumed = 0;
        for (const auto& l : lines) {
            consumed += l.textLength();
        }
        const bool overflowed = overflowedBeyondMax
            || consumed < description.length();

        qreal y = descY;
        for (int i = 0; i < lines.size(); ++i) {
            const auto& line = lines.at(i);
            const bool isLast = (i == lines.size() - 1);
            const QString segment = description.mid(
                line.textStart(), line.textLength());
            if (isLast && overflowed) {
                painter->drawText(
                    QRect(textRect.left(), int(y), textRect.width(), lineH),
                    Qt::AlignLeft | Qt::AlignVCenter,
                    sfm.elidedText(
                        description.mid(line.textStart()),
                        Qt::ElideRight, textRect.width()));
            } else {
                painter->drawText(
                    QRect(textRect.left(), int(y), textRect.width(), lineH),
                    Qt::AlignLeft | Qt::AlignVCenter, segment);
            }
            y += lineH;
        }
    }

    painter->restore();
}

} // namespace kinema::ui
