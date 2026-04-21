// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/TrayController.h"

#include "kinema_debug.h"

#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/PlayerWindow.h"
#endif

#include <KLocalizedString>
#include <KStatusNotifierItem>

#include <QAction>
#include <QIcon>
#include <QMenu>
#include <QSystemTrayIcon>
#include <QWidget>

namespace kinema::controllers {

TrayController::TrayController(QWidget* mainWindow, QObject* parent)
    : QObject(parent)
    , m_mainWindow(mainWindow)
{
    // Respect desktops that don't expose any tray at all (minimal
    // window managers, GNOME without extensions). KStatusNotifierItem
    // falls back from SNI to legacy QSystemTrayIcon automatically
    // when only the legacy mechanism is available, so this check
    // covers the genuinely-absent case.
    m_available = QSystemTrayIcon::isSystemTrayAvailable();
    if (!m_available) {
        qCInfo(KINEMA) << "no system tray host available;"
                       << "close-to-tray will be inert";
        return;
    }
    build();
}

void TrayController::setPlayerWindow(ui::player::PlayerWindow* window)
{
    m_playerWindow = window;
    refreshMenu();
}

void TrayController::build()
{
    m_tray = new KStatusNotifierItem(QStringLiteral("kinema"), this);

    // KStatusNotifierItem would by default auto-toggle the
    // associated window on activation and inject Minimize/Restore/
    // Quit standard actions. Disable both so our signals are the
    // single source of truth.
    m_tray->setAssociatedWindow(nullptr);
    m_tray->setStandardActionsEnabled(false);

    m_tray->setCategory(KStatusNotifierItem::ApplicationStatus);
    m_tray->setStatus(KStatusNotifierItem::Active);
    m_tray->setTitle(QStringLiteral("Kinema"));
    m_tray->setIconByName(QStringLiteral("dev.tlmtech.kinema"));
    m_tray->setToolTipIconByName(QStringLiteral("dev.tlmtech.kinema"));
    m_tray->setToolTipTitle(QStringLiteral("Kinema"));
    m_tray->setToolTipSubTitle(
        i18nc("@info:tooltip tray icon",
            "Cinema in motion \u2014 click to show or hide Kinema."));

    connect(m_tray, &KStatusNotifierItem::activateRequested,
        this, [this](bool /*active*/, const QPoint& /*pos*/) {
            Q_EMIT toggleMainWindowRequested();
        });

    auto* menu = m_tray->contextMenu();

    m_toggleAction = new QAction(this);
    connect(m_toggleAction, &QAction::triggered,
        this, &TrayController::toggleMainWindowRequested);
    menu->addAction(m_toggleAction);

#ifdef KINEMA_HAVE_LIBMPV
    m_showPlayerAction = new QAction(
        QIcon::fromTheme(QStringLiteral("media-playback-start")),
        i18nc("@action:inmenu tray", "Show Player"), this);
    connect(m_showPlayerAction, &QAction::triggered,
        this, &TrayController::showPlayerWindowRequested);
    menu->addAction(m_showPlayerAction);
#endif

    menu->addSeparator();

    auto* trayQuit = new QAction(
        QIcon::fromTheme(QStringLiteral("application-exit")),
        i18nc("@action:inmenu tray", "Quit Kinema"), this);
    connect(trayQuit, &QAction::triggered,
        this, &TrayController::quitRequested);
    menu->addAction(trayQuit);

    refreshMenu();
}

void TrayController::refreshMenu()
{
    if (!m_available) {
        return;
    }
    if (m_toggleAction && m_mainWindow) {
        const bool shown = m_mainWindow->isVisible()
            && !m_mainWindow->isMinimized();
        if (shown) {
            m_toggleAction->setText(
                i18nc("@action:inmenu tray", "Hide Kinema"));
            m_toggleAction->setIcon(
                QIcon::fromTheme(QStringLiteral("window-close")));
        } else {
            m_toggleAction->setText(
                i18nc("@action:inmenu tray", "Show Kinema"));
            m_toggleAction->setIcon(
                QIcon::fromTheme(QStringLiteral("window")));
        }
    }
#ifdef KINEMA_HAVE_LIBMPV
    if (m_showPlayerAction) {
        // Only meaningful when the player exists, has been used, and
        // is currently hidden.
        const bool canShow = m_playerWindow
            && m_playerWindow->hasEverLoaded()
            && !m_playerWindow->isVisible();
        m_showPlayerAction->setVisible(canShow);
    }
#endif
}

} // namespace kinema::controllers
