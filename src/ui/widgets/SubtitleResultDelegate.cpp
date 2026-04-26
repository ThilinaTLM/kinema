// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/SubtitleResultDelegate.h"

#include "ui/widgets/SubtitleResultsModel.h"

#include <KLocalizedString>

#include <QApplication>
#include <QFontMetrics>
#include <QLocale>
#include <QPainter>
#include <QPalette>
#include <QStyle>
#include <QStyleOptionButton>

namespace kinema::ui::widgets {

namespace {

constexpr int kRowPadding = 10;       // outer padding inside each row
constexpr int kBadgeSpacing = 6;
constexpr int kButtonWidth = 130;
constexpr int kButtonHeight = 28;
constexpr int kRowHeight = 64;

QString formatRating(double rating)
{
    if (rating <= 0.0) {
        return {};
    }
    return QStringLiteral(u"\u2605 %1").arg(rating, 0, 'f', 1);
}

QString formatDownloads(int n)
{
    return QLocale::system().toString(n);
}

void drawChip(QPainter* p, QRect& cursor, const QString& text,
    const QColor& fg, const QColor& bg, int height)
{
    QFontMetrics fm(p->font());
    const int textWidth = fm.horizontalAdvance(text);
    const int chipWidth = textWidth + 12;
    const QRect chip(cursor.left(), cursor.top(),
        chipWidth, height);
    p->save();
    p->setBrush(bg);
    p->setPen(Qt::NoPen);
    p->drawRoundedRect(chip, 4, 4);
    p->setPen(fg);
    p->drawText(chip, Qt::AlignCenter, text);
    p->restore();
    cursor.adjust(chipWidth + kBadgeSpacing, 0, 0, 0);
}

} // namespace

SubtitleResultDelegate::SubtitleResultDelegate(QObject* parent)
    : QStyledItemDelegate(parent)
{
}

QSize SubtitleResultDelegate::sizeHint(const QStyleOptionViewItem&,
    const QModelIndex&) const
{
    return { 480, kRowHeight };
}

QRect SubtitleResultDelegate::actionRectFor(const QStyleOptionViewItem& option,
    const QModelIndex&) const
{
    const QRect r = option.rect.adjusted(kRowPadding, 0, -kRowPadding, 0);
    const int x = r.right() - kButtonWidth + 1;
    const int y = r.top() + (r.height() - kButtonHeight) / 2;
    return { x, y, kButtonWidth, kButtonHeight };
}

QString SubtitleResultDelegate::actionLabelFor(const QModelIndex& index)
{
    const bool active = index.data(SubtitleResultsModel::ActiveRole).toBool();
    const bool cached = index.data(SubtitleResultsModel::CachedRole).toBool();
    if (active) {
        return i18nc("@action:button subtitle row action when already loaded "
                     "in the player",
            "Re-attach");
    }
    if (cached) {
        return i18nc("@action:button subtitle row action for cached entry",
            "Use");
    }
    return i18nc("@action:button subtitle row action for fresh download",
        "Download");
}

void SubtitleResultDelegate::paint(QPainter* painter,
    const QStyleOptionViewItem& option,
    const QModelIndex& index) const
{
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing, true);

    const auto palette = option.palette;
    const bool selected = option.state & QStyle::State_Selected;
    const bool hovered = option.state & QStyle::State_MouseOver;

    if (selected) {
        painter->fillRect(option.rect, palette.highlight());
    } else if (hovered) {
        auto hover = palette.color(QPalette::Highlight);
        hover.setAlpha(36);
        painter->fillRect(option.rect, hover);
    }

    // Bottom hairline separator — like the streams table's grid.
    painter->save();
    auto sep = palette.color(QPalette::Mid);
    sep.setAlpha(60);
    painter->setPen(sep);
    painter->drawLine(option.rect.bottomLeft(), option.rect.bottomRight());
    painter->restore();

    const QColor fg = selected
        ? palette.color(QPalette::HighlightedText)
        : palette.color(QPalette::Text);
    QColor dim = fg;
    dim.setAlphaF(0.65f);

    const QRect r = option.rect.adjusted(kRowPadding, 6, -kRowPadding, -6);

    // Reserve the action button on the right.
    const QRect button = actionRectFor(option, index);
    const QRect content(r.left(), r.top(),
        button.left() - r.left() - kRowPadding, r.height());

    // ---- Title row -------------------------------------------------------
    QFont titleFont = option.font;
    titleFont.setBold(true);
    painter->setFont(titleFont);
    QFontMetrics titleFm(titleFont);

    int badgeReserve = 0;
    const bool moviehash = index.data(
        SubtitleResultsModel::MoviehashMatchRole).toBool();
    const bool cached = index.data(SubtitleResultsModel::CachedRole).toBool();
    const bool active = index.data(SubtitleResultsModel::ActiveRole).toBool();
    if (moviehash) badgeReserve += titleFm.horizontalAdvance(QStringLiteral("🎯")) + kBadgeSpacing;
    if (cached)    badgeReserve += titleFm.horizontalAdvance(QStringLiteral("⏬")) + kBadgeSpacing;
    if (active)    badgeReserve += titleFm.horizontalAdvance(QStringLiteral("✓")) + kBadgeSpacing;

    const QRect titleRect(content.left(), content.top(),
        content.width() - badgeReserve, titleFm.height());

    QString title = index.data(SubtitleResultsModel::ReleaseRole).toString();
    if (title.isEmpty()) {
        title = index.data(SubtitleResultsModel::FileNameRole).toString();
    }
    painter->setPen(fg);
    painter->drawText(titleRect,
        Qt::AlignLeft | Qt::AlignVCenter,
        titleFm.elidedText(title, Qt::ElideRight, titleRect.width()));

    // Badges, painted right-to-left on the title row.
    int badgeRight = content.right();
    auto paintBadge = [&](const QString& glyph, const QColor& colour) {
        const int w = titleFm.horizontalAdvance(glyph);
        const QRect br(badgeRight - w, titleRect.top(), w, titleRect.height());
        painter->setPen(colour);
        painter->drawText(br, Qt::AlignVCenter | Qt::AlignRight, glyph);
        badgeRight -= w + kBadgeSpacing;
    };
    if (active)    paintBadge(QStringLiteral(u"\u2713"),
                       palette.color(QPalette::Highlight));
    if (cached)    paintBadge(QStringLiteral(u"\u2B07"), dim);
    if (moviehash) paintBadge(QStringLiteral(u"\U0001F3AF"), fg);

    // ---- Sub-line --------------------------------------------------------
    QFont subFont = option.font;
    subFont.setPointSizeF(qMax(7.0, subFont.pointSizeF() - 1.0));
    painter->setFont(subFont);
    QFontMetrics subFm(subFont);

    const QString lang = index.data(SubtitleResultsModel::LanguageRole).toString();
    const QString langName = index.data(SubtitleResultsModel::LanguageNameRole).toString();
    const QString format = index.data(SubtitleResultsModel::FormatRole).toString();
    const int downloads = index.data(SubtitleResultsModel::DownloadCountRole).toInt();
    const double rating = index.data(SubtitleResultsModel::RatingRole).toDouble();
    const bool hi = index.data(SubtitleResultsModel::HearingImpairedRole).toBool();
    const bool fpo = index.data(SubtitleResultsModel::ForeignPartsOnlyRole).toBool();

    const int subTop = content.bottom() - subFm.height() - 1;
    QRect cursor(content.left(), subTop, 0, subFm.height());

    // Language code chip (small monospace-ish box). Falls back to "??".
    {
        QFont mono = subFont;
        mono.setBold(true);
        painter->save();
        painter->setFont(mono);
        QColor chipBg = palette.color(QPalette::Highlight);
        chipBg.setAlphaF(0.18f);
        QColor chipFg = palette.color(QPalette::Highlight);
        if (selected) {
            chipBg = palette.color(QPalette::HighlightedText);
            chipBg.setAlphaF(0.25f);
            chipFg = palette.color(QPalette::HighlightedText);
        }
        const QString code = lang.isEmpty()
            ? QStringLiteral("??") : lang.toUpper();
        drawChip(painter, cursor, code, chipFg, chipBg, subFm.height());
        painter->restore();
    }

    // Plain text after the chip.
    QStringList parts;
    if (!langName.isEmpty()) parts << langName;
    if (!format.isEmpty())   parts << format;
    if (downloads > 0)       parts << i18ncp("@info downloads count",
                                     "%1 download", "%1 downloads",
                                     downloads, formatDownloads(downloads));
    const auto ratingText = formatRating(rating);
    if (!ratingText.isEmpty()) parts << ratingText;

    const QString sub = parts.join(QStringLiteral(u"  ·  "));
    painter->setPen(dim);
    QRect subTextRect(cursor.left(), subTop,
        content.right() - cursor.left(), subFm.height());
    // Reserve room for trailing chips before eliding the sub line.
    int trailingReserve = 0;
    if (hi)  trailingReserve += subFm.horizontalAdvance(QStringLiteral("HI"))  + 12 + kBadgeSpacing;
    if (fpo) trailingReserve += subFm.horizontalAdvance(QStringLiteral("FPO")) + 12 + kBadgeSpacing;
    subTextRect.setRight(subTextRect.right() - trailingReserve);
    painter->drawText(subTextRect,
        Qt::AlignLeft | Qt::AlignVCenter,
        subFm.elidedText(sub, Qt::ElideRight, subTextRect.width()));

    // Trailing HI / FPO chips on the sub-line.
    QRect chipCursor(subTextRect.right() + kBadgeSpacing, subTop, 0,
        subFm.height());
    if (hi) {
        QColor bg = palette.color(QPalette::Highlight);
        bg.setAlphaF(0.18f);
        drawChip(painter, chipCursor, QStringLiteral("HI"), fg, bg, subFm.height());
    }
    if (fpo) {
        QColor bg = palette.color(QPalette::Highlight);
        bg.setAlphaF(0.10f);
        drawChip(painter, chipCursor, QStringLiteral("FPO"), dim, bg, subFm.height());
    }

    // ---- Action button ---------------------------------------------------
    {
        QStyleOptionButton btn;
        btn.rect = button;
        btn.text = actionLabelFor(index);
        btn.state = QStyle::State_Enabled;
        if (hovered && button.contains(option.widget
                ? option.widget->mapFromGlobal(QCursor::pos())
                : QPoint())) {
            btn.state |= QStyle::State_MouseOver;
        }
        if (active) {
            // Tone down — re-attach is rarely the desired action.
            btn.state |= QStyle::State_AutoRaise;
        } else if (!cached) {
            // Highlight the primary action.
            btn.features = QStyleOptionButton::DefaultButton;
        }
        QStyle* style = option.widget ? option.widget->style()
                                      : QApplication::style();
        style->drawControl(QStyle::CE_PushButton, &btn, painter,
            option.widget);
    }

    painter->restore();
}

} // namespace kinema::ui::widgets
