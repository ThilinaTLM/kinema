// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QPushButton>

namespace kinema::ui::player::widgets {

class SkipChapterButton : public QPushButton
{
    Q_OBJECT
public:
    explicit SkipChapterButton(QWidget* parent = nullptr);

    void showLabel(const QString& text);
};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
