// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QWidget>

namespace kinema::ui::player::widgets {

/**
 * Centred modal-looking dim panel listing the keyboard bindings the
 * embedded player honours. Shown / hidden via `toggleVisibility()`
 * (PlayerOverlay calls it on `?` key presses and on the settings-menu
 * "Keyboard shortcuts" entry).
 *
 * Closes itself on `?`, `Esc`, or any click inside its own area \u2014
 * the parent overlay intercepts the key presses and forwards them
 * through `toggleVisibility()` / `hide()`.
 */
class CheatSheetOverlay : public QWidget
{
    Q_OBJECT
public:
    explicit CheatSheetOverlay(QWidget* parent = nullptr);

    void toggleVisibility();

protected:
    void paintEvent(QPaintEvent* e) override;
    void mousePressEvent(QMouseEvent* e) override;
    void keyPressEvent(QKeyEvent* e) override;
};

} // namespace kinema::ui::player::widgets
