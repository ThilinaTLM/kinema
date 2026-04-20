// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/SearchBar.h"

#include <KLocalizedString>

#include <QAction>
#include <QActionGroup>
#include <QHBoxLayout>
#include <QIcon>
#include <QLineEdit>
#include <QMenu>
#include <QToolButton>

namespace kinema::ui {

SearchBar::SearchBar(QWidget* parent)
    : QWidget(parent)
{
    m_edit = new QLineEdit(this);
    m_edit->setPlaceholderText(
        i18nc("@info:placeholder", "Search movies/series or paste an IMDB id (ttXXXXXXX)"));
    m_edit->setClearButtonEnabled(true);
    m_edit->addAction(QIcon::fromTheme(QStringLiteral("search")), QLineEdit::LeadingPosition);

    m_kindToggle = new QToolButton(this);
    m_kindToggle->setPopupMode(QToolButton::InstantPopup);
    m_kindToggle->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);

    auto* menu = new QMenu(m_kindToggle);
    auto* group = new QActionGroup(this);
    group->setExclusive(true);

    m_movieAction = menu->addAction(i18nc("@action search scope", "Movies"));
    m_movieAction->setCheckable(true);
    m_movieAction->setChecked(true);
    group->addAction(m_movieAction);
    connect(m_movieAction, &QAction::triggered, this, [this] {
        setMediaKind(api::MediaKind::Movie);
    });

    m_seriesAction = menu->addAction(i18nc("@action search scope", "TV Series"));
    m_seriesAction->setCheckable(true);
    group->addAction(m_seriesAction);
    connect(m_seriesAction, &QAction::triggered, this, [this] {
        setMediaKind(api::MediaKind::Series);
    });

    m_kindToggle->setMenu(menu);
    applyKindToButton();

    auto* searchBtn = new QToolButton(this);
    searchBtn->setIcon(QIcon::fromTheme(QStringLiteral("search")));
    searchBtn->setToolTip(i18nc("@info:tooltip", "Search"));
    searchBtn->setAutoRaise(true);

    auto* layout = new QHBoxLayout(this);
    layout->setContentsMargins(6, 4, 6, 4);
    layout->setSpacing(6);
    layout->addWidget(m_kindToggle);
    layout->addWidget(m_edit, 1);
    layout->addWidget(searchBtn);

    connect(m_edit, &QLineEdit::returnPressed, this, &SearchBar::emitRequest);
    connect(searchBtn, &QToolButton::clicked, this, &SearchBar::emitRequest);

    setFocusProxy(m_edit);
}

void SearchBar::setQuery(const QString& text)
{
    m_edit->setText(text);
}

void SearchBar::setMediaKind(api::MediaKind kind)
{
    if (m_kind == kind) {
        return;
    }
    m_kind = kind;
    if (m_movieAction && m_seriesAction) {
        (kind == api::MediaKind::Movie ? m_movieAction : m_seriesAction)
            ->setChecked(true);
    }
    applyKindToButton();
    Q_EMIT mediaKindChanged(m_kind);
}

void SearchBar::applyKindToButton()
{
    if (m_kind == api::MediaKind::Movie) {
        m_kindToggle->setIcon(QIcon::fromTheme(QStringLiteral("video-x-generic")));
        m_kindToggle->setText(i18nc("@action search scope", "Movies"));
    } else {
        m_kindToggle->setIcon(QIcon::fromTheme(QStringLiteral("view-media-recent")));
        m_kindToggle->setText(i18nc("@action search scope", "TV Series"));
    }
}

QString SearchBar::query() const
{
    return m_edit->text().trimmed();
}

void SearchBar::emitRequest()
{
    const auto q = query();
    if (q.isEmpty()) {
        return;
    }
    Q_EMIT queryRequested(q, m_kind);
}

} // namespace kinema::ui
