// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/LanguageChipBar.h"

#include "core/Language.h"

#include <KLocalizedString>

#include <QFrame>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QMenu>
#include <QToolButton>

namespace kinema::ui::widgets {

namespace {

/// One non-removable chip: `EN  English  ✕`. Constructed with the
/// code; emits `removeRequested` when the close button is clicked.
class Chip : public QFrame
{
    Q_OBJECT
public:
    Chip(const QString& code, const QString& display, QWidget* parent = nullptr)
        : QFrame(parent)
    {
        setFrameShape(QFrame::StyledPanel);
        setObjectName(QStringLiteral("LanguageChip"));
        // Inline stylesheet keeps the chip visually consistent across
        // Breeze / Adwaita without needing per-app theming.
        setStyleSheet(QStringLiteral(
            "QFrame#LanguageChip { border-radius: 10px;"
            " background: palette(highlight);"
            " color: palette(highlighted-text); padding: 0 6px; }"
            " QFrame#LanguageChip QLabel { color: palette(highlighted-text); }"
            " QFrame#LanguageChip QToolButton { border: none; padding: 0;"
            " color: palette(highlighted-text); }"));

        auto* row = new QHBoxLayout(this);
        row->setContentsMargins(8, 2, 4, 2);
        row->setSpacing(6);

        auto* codeLabel = new QLabel(code.toUpper(), this);
        auto codeFont = codeLabel->font();
        codeFont.setBold(true);
        codeLabel->setFont(codeFont);
        row->addWidget(codeLabel);

        if (!display.isEmpty() && display.compare(code, Qt::CaseInsensitive) != 0) {
            auto* nameLabel = new QLabel(display, this);
            row->addWidget(nameLabel);
        }

        auto* close = new QToolButton(this);
        close->setIcon(QIcon::fromTheme(QStringLiteral("window-close-symbolic"),
            QIcon::fromTheme(QStringLiteral("dialog-close"))));
        close->setAutoRaise(true);
        close->setToolTip(i18nc("@info:tooltip", "Remove this language"));
        connect(close, &QToolButton::clicked, this,
            [this] { Q_EMIT removeRequested(); });
        row->addWidget(close);
    }

Q_SIGNALS:
    void removeRequested();
};

} // namespace

LanguageChipBar::LanguageChipBar(QWidget* parent)
    : QWidget(parent)
{
    m_row = new QHBoxLayout(this);
    m_row->setContentsMargins(0, 0, 0, 0);
    m_row->setSpacing(6);

    m_addButton = new QToolButton(this);
    m_addButton->setText(i18nc("@action:button", "+ Add"));
    m_addButton->setPopupMode(QToolButton::InstantPopup);
    m_addButton->setToolButtonStyle(Qt::ToolButtonTextOnly);
    m_addButton->setAutoRaise(false);
    m_addButton->setToolTip(i18nc("@info:tooltip",
        "Add a language to the search filter"));
    connect(m_addButton, &QToolButton::clicked, this,
        &LanguageChipBar::showAddMenu);

    m_row->addStretch(1);
    m_row->addWidget(m_addButton);
}

void LanguageChipBar::setLanguages(const QStringList& codes)
{
    QStringList normalised;
    normalised.reserve(codes.size());
    for (const auto& c : codes) {
        const auto lo = c.trimmed().toLower();
        if (lo.size() == 3 && !normalised.contains(lo)) {
            normalised.append(lo);
        }
    }
    if (normalised == m_codes) {
        return;
    }
    m_codes = std::move(normalised);
    rebuildChips();
    Q_EMIT languagesChanged(m_codes);
}

void LanguageChipBar::addLanguage(const QString& code)
{
    const auto lo = code.toLower();
    if (lo.size() != 3 || m_codes.contains(lo)) {
        return;
    }
    m_codes.append(lo);
    rebuildChips();
    Q_EMIT languagesChanged(m_codes);
}

void LanguageChipBar::removeLanguage(const QString& code)
{
    if (m_codes.removeAll(code.toLower()) > 0) {
        rebuildChips();
        Q_EMIT languagesChanged(m_codes);
    }
}

void LanguageChipBar::rebuildChips()
{
    // Pull the existing chips off the layout while leaving the
    // trailing stretch + add button in place.
    while (m_row->count() > 2) {
        auto* item = m_row->takeAt(0);
        if (auto* w = item->widget()) {
            w->deleteLater();
        }
        delete item;
    }

    const auto common = core::language::commonLanguages();
    auto display = [&](const QString& code) {
        for (const auto& c : common) {
            if (c.code.compare(code, Qt::CaseInsensitive) == 0) {
                return c.display;
            }
        }
        return core::language::displayName(code);
    };

    int index = 0;
    for (const auto& code : m_codes) {
        auto* chip = new Chip(code, display(code), this);
        connect(chip, &Chip::removeRequested, this,
            [this, code] { removeLanguage(code); });
        m_row->insertWidget(index++, chip);
    }
}

void LanguageChipBar::showAddMenu()
{
    QMenu menu(this);
    int added = 0;
    for (const auto& opt : core::language::commonLanguages()) {
        if (m_codes.contains(opt.code)) {
            continue;
        }
        auto* act = menu.addAction(QStringLiteral("%1 (%2)")
                .arg(opt.display, opt.code));
        const auto code = opt.code;
        connect(act, &QAction::triggered, this,
            [this, code] { addLanguage(code); });
        ++added;
    }
    if (added == 0) {
        auto* act = menu.addAction(i18nc("@info no more languages to pick",
            "All common languages added"));
        act->setEnabled(false);
    }
    menu.exec(m_addButton->mapToGlobal(QPoint(0, m_addButton->height())));
}

} // namespace kinema::ui::widgets

#include "LanguageChipBar.moc"
