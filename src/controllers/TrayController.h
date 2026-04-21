// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>

class KStatusNotifierItem;
class QAction;
class QWidget;

namespace kinema::ui::player {
class PlayerWindow;
}

namespace kinema::controllers {

/**
 * Owns the KStatusNotifierItem and its context menu. Lifts tray-related
 * state out of MainWindow so the main window can focus on layout +
 * action wiring.
 *
 * TrayController does not decide whether to hide/show windows itself \u2014
 * it emits requests that MainWindow translates into window operations.
 * This keeps window lifecycle decisions (close-to-tray rules, quit
 * path) in one place.
 *
 * `available()` reflects QSystemTrayIcon availability at construction.
 * When false, the menu is never built and the request signals are
 * never emitted. Callers can still call every slot without harm.
 */
class TrayController : public QObject
{
    Q_OBJECT
public:
    /// `mainWindow` is used only to query visibility state for the
    /// Show/Hide menu label. TrayController does NOT take ownership.
    TrayController(QWidget* mainWindow, QObject* parent = nullptr);

    bool available() const noexcept { return m_available; }

    /// Hook up an embedded-player window so the tray can surface a
    /// "Show Player" entry when the window exists but is hidden.
    /// Safe to call with nullptr to detach.
    void setPlayerWindow(ui::player::PlayerWindow* window);

public Q_SLOTS:
    /// Rebuild the context menu text/visibility to match current
    /// main-window / player-window state. MainWindow calls this
    /// after any visibility change.
    void refreshMenu();

Q_SIGNALS:
    /// User clicked the tray icon or its toggle entry.
    void toggleMainWindowRequested();

    /// User clicked "Show Player" in the tray menu.
    void showPlayerWindowRequested();

    /// User clicked "Quit Kinema" in the tray menu.
    void quitRequested();

private:
    void build();

    QWidget* m_mainWindow;
    bool m_available = false;

    KStatusNotifierItem* m_tray {};
    QAction* m_toggleAction {};
    QAction* m_showPlayerAction {};

    ui::player::PlayerWindow* m_playerWindow {};
};

} // namespace kinema::controllers
