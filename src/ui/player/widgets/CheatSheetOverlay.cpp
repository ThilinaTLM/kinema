// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/widgets/CheatSheetOverlay.h"

#include <KLocalizedString>

#include <QColor>
#include <QGridLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QVBoxLayout>

namespace kinema::ui::player::widgets {

namespace {

struct Row {
    QString keys;
    QString action;
};

/**
 * Hard-coded binding list. Kept in sync by hand with
 * `data/kinema/mpv/input.conf` + PlayerWindow / MpvWidget / overlay
 * key handlers. The overview plan notes rows marked "(M3)" are
 * stubbed until the series controller lands; show them anyway so the
 * user isn't surprised by the future behaviour.
 */
QList<Row> bindingRows()
{
    return {
        { i18nc("@label cheat sheet key", "Space / Click"),
          i18nc("@label cheat sheet action", "Play / Pause") },
        { i18nc("@label cheat sheet key", "\u2190  \u2192"),
          i18nc("@label cheat sheet action", "Seek \u00b15 s") },
        { i18nc("@label cheat sheet key",
              "Shift+\u2190 / Shift+\u2192"),
          i18nc("@label cheat sheet action", "Seek \u00b160 s") },
        { i18nc("@label cheat sheet key", "M"),
          i18nc("@label cheat sheet action", "Mute") },
        { i18nc("@label cheat sheet key", "J"),
          i18nc("@label cheat sheet action", "Cycle subtitles") },
        { i18nc("@label cheat sheet key", "#"),
          i18nc("@label cheat sheet action", "Cycle audio track") },
        { i18nc("@label cheat sheet key", "[ / ]"),
          i18nc("@label cheat sheet action", "Speed \u2212 / +") },
        { i18nc("@label cheat sheet key", "Backspace"),
          i18nc("@label cheat sheet action", "Reset speed") },
        { i18nc("@label cheat sheet key", "F / Double-click"),
          i18nc("@label cheat sheet action", "Toggle fullscreen") },
        { i18nc("@label cheat sheet key", "Esc"),
          i18nc("@label cheat sheet action",
              "Leave fullscreen / close") },
        { i18nc("@label cheat sheet key", "I"),
          i18nc("@label cheat sheet action", "Stats overlay") },
        { i18nc("@label cheat sheet key", "N"),
          i18nc("@label cheat sheet action", "Play next episode now") },
        { i18nc("@label cheat sheet key", "Shift+N"),
          i18nc("@label cheat sheet action", "Cancel auto-next") },
        { i18nc("@label cheat sheet key", "?"),
          i18nc("@label cheat sheet action", "This help") },
    };
}

} // namespace

CheatSheetOverlay::CheatSheetOverlay(QWidget* parent)
    : QWidget(parent)
{
    setAttribute(Qt::WA_NoSystemBackground);
    setAutoFillBackground(false);
    setFocusPolicy(Qt::StrongFocus);
    hide();

    auto* title = new QLabel(i18nc("@title panel heading",
        "Keyboard shortcuts"), this);
    title->setStyleSheet(QStringLiteral(
        "QLabel { color: white; font-size: 16px; font-weight: 700; }"));
    title->setAlignment(Qt::AlignCenter);

    auto* grid = new QGridLayout;
    grid->setContentsMargins(0, 0, 0, 0);
    grid->setHorizontalSpacing(24);
    grid->setVerticalSpacing(4);

    const auto rows = bindingRows();
    for (int i = 0; i < rows.size(); ++i) {
        auto* k = new QLabel(rows[i].keys, this);
        k->setStyleSheet(QStringLiteral(
            "QLabel {"
            "  color: white;"
            "  font-family: monospace;"
            "  background: rgba(255, 255, 255, 25);"
            "  padding: 1px 6px;"
            "  border-radius: 3px;"
            "}"));
        auto* v = new QLabel(rows[i].action, this);
        v->setStyleSheet(QStringLiteral(
            "QLabel { color: rgba(255, 255, 255, 220); }"));
        grid->addWidget(k, i, 0, Qt::AlignRight | Qt::AlignVCenter);
        grid->addWidget(v, i, 1, Qt::AlignLeft  | Qt::AlignVCenter);
    }

    auto* outer = new QVBoxLayout(this);
    outer->setContentsMargins(32, 24, 32, 24);
    outer->addStretch();
    outer->addWidget(title, 0, Qt::AlignHCenter);
    outer->addSpacing(12);
    outer->addLayout(grid);
    outer->addStretch();
}

void CheatSheetOverlay::toggleVisibility()
{
    if (isVisible()) {
        hide();
    } else {
        show();
        raise();
        setFocus(Qt::OtherFocusReason);
    }
}

void CheatSheetOverlay::paintEvent(QPaintEvent* /*e*/)
{
    // Dim the whole video, draw a rounded centred panel with slightly
    // stronger alpha. Panel is rect() deflated by the layout margins.
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);
    p.fillRect(rect(), QColor(0, 0, 0, 160));

    // Centred backing behind the content. The layout centres the
    // block vertically with stretch; approximate the block rect from
    // the child positions.
    const QMargins m(32, 24, 32, 24);
    QRect r = rect().adjusted(m.left(), m.top(),
        -m.right(), -m.bottom());
    // Shrink to a reasonable max width so it reads as a modal block.
    constexpr int kMaxWidth = 520;
    if (r.width() > kMaxWidth) {
        const int shrink = (r.width() - kMaxWidth) / 2;
        r.adjust(shrink, 0, -shrink, 0);
    }
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(15, 15, 15, 220));
    p.drawRoundedRect(r, 10, 10);
}

void CheatSheetOverlay::mousePressEvent(QMouseEvent* /*e*/)
{
    hide();
}

void CheatSheetOverlay::keyPressEvent(QKeyEvent* e)
{
    if (e->key() == Qt::Key_Escape || e->key() == Qt::Key_Question) {
        hide();
        e->accept();
        return;
    }
    QWidget::keyPressEvent(e);
}

} // namespace kinema::ui::player::widgets
