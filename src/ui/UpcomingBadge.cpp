// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/UpcomingBadge.h"

#include "core/DateFormat.h"

#include <KLocalizedString>

#include <QColor>
#include <QFont>
#include <QFontMetrics>
#include <QLabel>
#include <QPainter>
#include <QPalette>
#include <QRect>
#include <QString>

namespace kinema::ui {

namespace {

constexpr int kHPad = 8;
constexpr int kVPad = 2;
constexpr int kRadius = 8;

QString badgeText(const QDate& released)
{
    return i18nc("@label upcoming-release pill. %1 is a formatted date",
        "Upcoming · %1", core::formatReleaseDate(released));
}

QFont badgeFont(const QFont& base)
{
    QFont f = base;
    f.setBold(true);
    // Slightly smaller than the surrounding text so the pill reads
    // as a tag, not as a header.
    f.setPointSizeF(base.pointSizeF() * 0.85);
    return f;
}

} // namespace

QLabel* makeUpcomingBadge(QWidget* parent)
{
    auto* lbl = new QLabel(parent);
    lbl->setObjectName(QStringLiteral("kinemaUpcomingBadge"));
    auto f = badgeFont(lbl->font());
    lbl->setFont(f);
    lbl->setAlignment(Qt::AlignCenter);
    lbl->setContentsMargins(kHPad, kVPad, kHPad, kVPad);
    // Stylesheet keeps the pill shape regardless of global style.
    // Palette-derived colors would be nicer but Qt stylesheets don't
    // expand `palette(highlight)` reliably across all themes; use a
    // tinted accent that reads well on both light and dark backgrounds.
    lbl->setStyleSheet(QStringLiteral(
        "QLabel#kinemaUpcomingBadge {"
        " background-color: rgba(255, 170, 0, 0.22);"
        " color: palette(highlighted-text);"
        " border: 1px solid rgba(255, 170, 0, 0.55);"
        " border-radius: %1px;"
        " padding: %2px %3px;"
        "}")
            .arg(kRadius)
            .arg(kVPad)
            .arg(kHPad));
    lbl->hide();
    return lbl;
}

void setUpcomingBadgeDate(QLabel* badge, const QDate& released)
{
    if (!badge) {
        return;
    }
    badge->setText(badgeText(released));
}

QSize upcomingBadgeSize(const QDate& released, const QFont& baseFont)
{
    const QFontMetrics fm(badgeFont(baseFont));
    const int w = fm.horizontalAdvance(badgeText(released)) + 2 * kHPad;
    const int h = fm.height() + 2 * kVPad;
    return { w, h };
}

void paintUpcomingBadge(QPainter* painter, const QRect& rect,
    const QDate& released, const QPalette& palette, const QFont& baseFont)
{
    if (!painter) {
        return;
    }
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing, true);

    // Warm amber accent, pulled out of the stylesheet for the painter
    // path. Slightly stronger opacity than the widget variant because
    // there's no border under the selection highlight.
    QColor bg(255, 170, 0);
    bg.setAlphaF(0.28f);
    QColor border(255, 170, 0);
    border.setAlphaF(0.65f);

    painter->setBrush(bg);
    painter->setPen(border);
    painter->drawRoundedRect(rect, kRadius, kRadius);

    painter->setPen(palette.color(QPalette::Text));
    painter->setFont(badgeFont(baseFont));
    painter->drawText(rect, Qt::AlignCenter, badgeText(released));
    painter->restore();
}

} // namespace kinema::ui
