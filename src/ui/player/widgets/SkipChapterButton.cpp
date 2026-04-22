// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/SkipChapterButton.h"

namespace kinema::ui::player::widgets {

SkipChapterButton::SkipChapterButton(QWidget* parent)
    : QPushButton(parent)
{
    setFocusPolicy(Qt::NoFocus);
    setCursor(Qt::PointingHandCursor);
    setStyleSheet(QStringLiteral(
        "QPushButton {"
        "  color: white;"
        "  background: rgba(35, 35, 35, 228);"
        "  border: 1px solid rgba(255, 255, 255, 32);"
        "  border-radius: 16px;"
        "  padding: 5px 12px;"
        "  font-weight: 600;"
        "}"
        "QPushButton:hover {"
        "  background: rgba(52, 52, 52, 235);"
        "}"
        "QPushButton:pressed {"
        "  background: rgba(24, 24, 24, 235);"
        "}"));
    hide();
}

void SkipChapterButton::showLabel(const QString& text)
{
    setText(text);
    show();
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
