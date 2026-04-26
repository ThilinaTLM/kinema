// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QStringList>
#include <QToolButton>

class QMenu;

namespace kinema::ui::widgets {

/**
 * Compact native language multi-picker.
 *
 * Renders as a single `QToolButton` with `InstantPopup`; the popup
 * is a `QMenu` of checkable common-language entries pulled from
 * `core::language::commonLanguages()`. Toggling an entry adds /
 * removes the ISO 639-2 code; the button text summarises the
 * selection (`"EN, FR, DE"` or `"5 languages"` when truncation
 * would be needed). The full list is always available via the
 * tooltip.
 *
 * Public API mirrors the deleted `LanguageChipBar` so the call site
 * in `SubtitlesDialog` only has to swap the type.
 */
class LanguagePickerButton : public QToolButton
{
    Q_OBJECT
public:
    explicit LanguagePickerButton(QWidget* parent = nullptr);

    /// Currently selected ISO 639-2 codes, in display order.
    QStringList languages() const { return m_codes; }

public Q_SLOTS:
    /// Replace the selection. Normalises to lower-case 3-letter
    /// codes, drops duplicates, and emits `languagesChanged` only
    /// when the result actually changed.
    void setLanguages(const QStringList& codes);

Q_SIGNALS:
    void languagesChanged(const QStringList& codes);

private:
    void rebuildMenu();
    void refreshButton();

    QStringList m_codes;
    QMenu* m_menu {};
};

} // namespace kinema::ui::widgets
