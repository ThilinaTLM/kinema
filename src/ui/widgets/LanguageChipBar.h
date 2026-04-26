// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QStringList>
#include <QWidget>

class QHBoxLayout;
class QToolButton;

namespace kinema::ui::widgets {

/**
 * Editable list of ISO 639-2 language codes shown as removable
 * chips with a trailing "+ Add" button. Used by the subtitles
 * dialog filter strip.
 *
 * The widget owns the language list state. Mutations (chip removal,
 * "+ Add" pick) emit `languagesChanged(QStringList)`. Codes are
 * normalised to lowercase on insertion, deduplicated, and order is
 * preserved.
 *
 * The "+ Add" popup pulls choices from `core::language::commonLanguages`.
 * Already-selected codes are filtered out of the menu; once the menu
 * empties it reads "All added".
 */
class LanguageChipBar : public QWidget
{
    Q_OBJECT
public:
    explicit LanguageChipBar(QWidget* parent = nullptr);

    QStringList languages() const { return m_codes; }

public Q_SLOTS:
    /// Replace the chip set. Emits `languagesChanged` only if the
    /// list actually changed.
    void setLanguages(const QStringList& codes);

Q_SIGNALS:
    void languagesChanged(const QStringList& codes);

private:
    void rebuildChips();
    void addLanguage(const QString& code);
    void removeLanguage(const QString& code);
    void showAddMenu();

    QStringList m_codes;
    QHBoxLayout* m_row {};
    QToolButton* m_addButton {};
};

} // namespace kinema::ui::widgets
