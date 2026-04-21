// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QDate>

class QLabel;
class QPainter;
class QPalette;
class QRect;
class QWidget;

namespace kinema::ui {

/**
 * "Upcoming · 17 Jun 2027" pill used to flag unreleased movies,
 * series and episodes.
 *
 * Two flavours:
 *   - widget: `makeUpcomingBadge(parent)` returns a styled QLabel for
 *     embedding in layouts (DetailPane / SeriesDetailPane headers).
 *   - painter: `paintUpcomingBadge(...)` draws the same visual into a
 *     QPainter, for item-view delegates (EpisodeDelegate).
 *
 * Colors are pulled from the active palette's Highlight role so the
 * badge stays readable under both light and dark themes.
 */
QLabel* makeUpcomingBadge(QWidget* parent = nullptr);

/// Set / update the date shown on a badge built by `makeUpcomingBadge`.
void setUpcomingBadgeDate(QLabel* badge, const QDate& released);

/// Size the painter-variant pill would occupy for a given release date,
/// using the supplied base font. Returned size already includes the
/// rounded-rect horizontal padding.
QSize upcomingBadgeSize(const QDate& released, const QFont& baseFont);

/// Paint the pill into `rect` (expected to match `upcomingBadgeSize`).
void paintUpcomingBadge(QPainter* painter, const QRect& rect,
    const QDate& released, const QPalette& palette, const QFont& baseFont);

} // namespace kinema::ui
