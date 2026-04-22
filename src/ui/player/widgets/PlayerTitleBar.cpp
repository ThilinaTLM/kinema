// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/PlayerTitleBar.h"

#include "ui/player/widgets/PlayerMenus.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLinearGradient>
#include <QMenu>
#include <QPainter>
#include <QToolButton>
#include <QVBoxLayout>

namespace kinema::ui::player::widgets {

namespace {

QToolButton* makeIconButton(const QString& iconName,
    const QString& tooltip, QWidget* parent)
{
    auto* b = new QToolButton(parent);
    b->setIcon(QIcon::fromTheme(iconName));
    b->setToolTip(tooltip);
    b->setAutoRaise(true);
    b->setIconSize(QSize(22, 22));
    b->setFocusPolicy(Qt::NoFocus);
    return b;
}

} // namespace

PlayerTitleBar::PlayerTitleBar(QWidget* parent)
    : QWidget(parent)
{
    // Sub-widgets receive clicks; the bar itself renders only the
    // gradient backing.
    setAttribute(Qt::WA_TransparentForMouseEvents, false);
    setAutoFillBackground(false);

    m_backButton = makeIconButton(
        QStringLiteral("go-previous"),
        i18nc("@info:tooltip", "Back to Kinema (Esc)"),
        this);
    connect(m_backButton, &QToolButton::clicked,
        this, &PlayerTitleBar::closeRequested);

    m_titleLabel = new QLabel(this);
    m_titleLabel->setStyleSheet(QStringLiteral(
        "QLabel { color: white; font-weight: 600; font-size: 14px; }"));
    m_titleLabel->setTextInteractionFlags(Qt::NoTextInteraction);

    m_subtitleLabel = new QLabel(this);
    m_subtitleLabel->setStyleSheet(QStringLiteral(
        "QLabel { color: rgba(255, 255, 255, 180); font-size: 11px; }"));
    m_subtitleLabel->setTextInteractionFlags(Qt::NoTextInteraction);
    m_subtitleLabel->setVisible(false);

    auto* textColumn = new QVBoxLayout;
    textColumn->setContentsMargins(0, 0, 0, 0);
    textColumn->setSpacing(0);
    textColumn->addWidget(m_titleLabel);
    textColumn->addWidget(m_subtitleLabel);

    m_settingsButton = makeIconButton(
        QStringLiteral("configure"),
        i18nc("@info:tooltip", "Player settings"),
        this);
    m_settingsButton->setPopupMode(QToolButton::InstantPopup);
    connect(m_settingsButton, &QToolButton::clicked,
        this, &PlayerTitleBar::onSettingsButtonClicked);

    m_closeButton = makeIconButton(
        QStringLiteral("window-close"),
        i18nc("@info:tooltip", "Close player"),
        this);
    connect(m_closeButton, &QToolButton::clicked,
        this, &PlayerTitleBar::closeRequested);

    auto* lay = new QHBoxLayout(this);
    lay->setContentsMargins(8, 4, 8, 4);
    lay->setSpacing(8);
    lay->addWidget(m_backButton);
    lay->addLayout(textColumn, /*stretch=*/1);
    lay->addWidget(m_settingsButton);
    lay->addWidget(m_closeButton);
}

void PlayerTitleBar::setContext(const api::PlaybackContext& ctx)
{
    // Movies: main title + empty subtitle. Series: series title +
    // "SxxEyy — episode title". Fall back to the generic `title`
    // when `seriesTitle` isn't filled.
    const bool isSeries = ctx.key.kind == api::MediaKind::Series
        && ctx.key.season && ctx.key.episode;
    const QString primary = isSeries && !ctx.seriesTitle.isEmpty()
        ? ctx.seriesTitle
        : ctx.title;
    m_titleLabel->setText(primary);

    if (isSeries) {
        const int s = *ctx.key.season;
        const int e = *ctx.key.episode;
        const QString label = ctx.episodeTitle.isEmpty()
            ? i18nc(
                "@label:bold episode code without title, e.g. \"S1E4\"",
                "S%1E%2",
                QString::number(s), QString::number(e))
            : i18nc(
                "@label episode code and title, e.g. "
                "\"S1E4 \u2014 The Beginning\"",
                "S%1E%2 \u2014 %3",
                QString::number(s), QString::number(e),
                ctx.episodeTitle);
        m_subtitleLabel->setText(label);
        m_subtitleLabel->setVisible(true);
    } else {
        m_subtitleLabel->clear();
        m_subtitleLabel->setVisible(false);
    }
}

void PlayerTitleBar::setTrackList(const core::tracks::TrackList& tracks)
{
    m_tracks = tracks;
    // The menu is rebuilt on demand when the gear is clicked, so
    // nothing visual changes here. Stored value is authoritative.
}

void PlayerTitleBar::setSpeed(double factor)
{
    m_speed = factor;
}

void PlayerTitleBar::onSettingsButtonClicked()
{
    // Build a fresh menu on every click \u2014 track lists can change
    // between presses (language swap, external sub attached), so
    // re-synthesising the submenu tree is both simpler and strictly
    // correct.
    QMenu menu(this);

    auto* audioMenu = menu.addMenu(
        i18nc("@title:menu audio track picker", "Audio"));
    menus::populateAudioMenu(audioMenu, m_tracks,
        [this](int id) { Q_EMIT audioTrackRequested(id); });

    auto* subMenu = menu.addMenu(
        i18nc("@title:menu subtitle picker", "Subtitles"));
    menus::populateSubtitleMenu(subMenu, m_tracks,
        [this](int id) { Q_EMIT subtitleTrackRequested(id); });

    auto* speedMenu = menu.addMenu(
        i18nc("@title:menu playback speed picker", "Playback speed"));
    menus::populateSpeedMenu(speedMenu, m_speed,
        [this](double s) { Q_EMIT speedRequested(s); });

    menu.addSeparator();
    auto* statsAction = menu.addAction(i18nc(
        "@action:inmenu toggles the stats overlay", "Video statistics"));
    connect(statsAction, &QAction::triggered,
        this, &PlayerTitleBar::statsToggleRequested);

    auto* helpAction = menu.addAction(i18nc(
        "@action:inmenu opens the keybinding cheat sheet",
        "Keyboard shortcuts"));
    connect(helpAction, &QAction::triggered,
        this, &PlayerTitleBar::cheatSheetToggleRequested);

    // Anchor below the gear button.
    menu.exec(m_settingsButton->mapToGlobal(
        QPoint(0, m_settingsButton->height())));
}

void PlayerTitleBar::paintEvent(QPaintEvent* /*e*/)
{
    QPainter p(this);
    QLinearGradient g(rect().topLeft(), rect().bottomLeft());
    g.setColorAt(0.0, QColor(0, 0, 0, 200));
    g.setColorAt(0.6, QColor(0, 0, 0, 140));
    g.setColorAt(1.0, QColor(0, 0, 0, 0));
    p.fillRect(rect(), g);
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
