// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/LanguagePickerButton.h"

#include "core/Language.h"

#include <KLocalizedString>

#include <QAction>
#include <QFontMetrics>
#include <QIcon>
#include <QMenu>
#include <QSignalBlocker>
#include <QStringList>

namespace kinema::ui::widgets {

namespace {

constexpr int kMaxInlineCodes = 3;

} // namespace

LanguagePickerButton::LanguagePickerButton(QWidget* parent)
    : QToolButton(parent)
{
    setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    setIcon(QIcon::fromTheme(QStringLiteral("preferences-desktop-locale"),
        QIcon::fromTheme(QStringLiteral("flag"))));
    setPopupMode(QToolButton::InstantPopup);
    setAutoRaise(false);

    m_menu = new QMenu(this);
    setMenu(m_menu);

    rebuildMenu();
    refreshButton();
}

void LanguagePickerButton::setLanguages(const QStringList& codes)
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

    // Sync menu check state without re-emitting toggles back into
    // ourselves.
    {
        QSignalBlocker blocker(m_menu);
        for (auto* a : m_menu->actions()) {
            const auto code = a->data().toString();
            if (!code.isEmpty()) {
                a->setChecked(m_codes.contains(code));
            }
        }
    }
    refreshButton();
    Q_EMIT languagesChanged(m_codes);
}

void LanguagePickerButton::rebuildMenu()
{
    m_menu->clear();
    for (const auto& opt : core::language::commonLanguages()) {
        auto* act = m_menu->addAction(QStringLiteral("%1 (%2)")
                .arg(opt.display, opt.code.toUpper()));
        act->setCheckable(true);
        act->setData(opt.code.toLower());
        act->setChecked(m_codes.contains(opt.code.toLower()));

        const QString code = opt.code.toLower();
        connect(act, &QAction::toggled, this, [this, code](bool on) {
            QStringList next = m_codes;
            if (on) {
                if (!next.contains(code)) {
                    next.append(code);
                }
            } else {
                next.removeAll(code);
            }
            setLanguages(next);
        });
    }

    // Disable the language filter entirely. Empty selection is
    // already a valid "no filter" state in the controller; this
    // entry just makes it reachable in one click instead of
    // unchecking every entry by hand.
    m_menu->addSeparator();
    m_clearAction = m_menu->addAction(
        i18nc("@action language picker, drop selection",
            "Clear (any language)"));
    connect(m_clearAction, &QAction::triggered, this, [this] {
        setLanguages({});
    });
}

void LanguagePickerButton::refreshButton()
{
    const int n = m_codes.size();
    if (n == 0) {
        setText(i18nc("@action:button language picker, no selection",
            "No languages"));
    } else if (n <= kMaxInlineCodes) {
        QStringList upper;
        upper.reserve(n);
        for (const auto& c : m_codes) {
            upper << c.toUpper();
        }
        setText(upper.join(QStringLiteral(", ")));
    } else {
        setText(i18ncp("@action:button language picker, count summary",
            "%1 language", "%1 languages", n));
    }

    if (m_clearAction) {
        m_clearAction->setEnabled(n > 0);
    }

    // Tooltip always shows the full localised list.
    QStringList names;
    names.reserve(m_codes.size());
    for (const auto& c : m_codes) {
        names << QStringLiteral("%1 (%2)")
                     .arg(core::language::displayName(c), c.toUpper());
    }
    setToolTip(names.isEmpty()
            ? i18nc("@info:tooltip language picker, empty",
                  "Pick the languages to search for")
            : names.join(QStringLiteral("\n")));
}

} // namespace kinema::ui::widgets
