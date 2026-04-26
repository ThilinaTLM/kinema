// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/MainController.h"

#include <QTest>

using kinema::ui::qml::MainController;

class TestMainControllerCloseLogic : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    /// `requestQuit()` was called → unconditionally accept the
    /// close, never hide, never toast. Covers the path where
    /// the user picked the drawer's Quit action or `Ctrl+Q`.
    void reallyQuitAcceptsClose();

    /// User has disabled close-to-tray in settings → close is
    /// the genuine quit path, ignore tray availability.
    void closeToTrayDisabledAcceptsClose();

    /// Setting is on but tray host isn't available (minimal WM,
    /// GNOME-without-extensions, …) → fall back to "close = quit"
    /// so the app doesn't disappear into a tray that doesn't exist.
    void noTrayAvailableAcceptsClose();

    /// Happy path: close-to-tray on, tray available, no quit
    /// requested. The window is hidden and the first hide emits
    /// the toast.
    void hideToTrayFirstTime();

    /// Subsequent hides on the same session must NOT re-emit the
    /// toast. The flag is one-shot per process lifetime.
    void hideToTrayDoesNotRepeatToast();
};

void TestMainControllerCloseLogic::reallyQuitAcceptsClose()
{
    // Even with every "should hide" knob on, reallyQuit wins.
    auto d = MainController::evaluateCloseRequest(
        /*reallyQuit=*/true, /*closeToTrayPref=*/true,
        /*trayAvail=*/true, /*toastShown=*/false);
    QVERIFY(d.acceptClose);
    QVERIFY(!d.hideWindow);
    QVERIFY(!d.emitToast);
}

void TestMainControllerCloseLogic::closeToTrayDisabledAcceptsClose()
{
    auto d = MainController::evaluateCloseRequest(
        /*reallyQuit=*/false, /*closeToTrayPref=*/false,
        /*trayAvail=*/true, /*toastShown=*/false);
    QVERIFY(d.acceptClose);
    QVERIFY(!d.hideWindow);
    QVERIFY(!d.emitToast);
}

void TestMainControllerCloseLogic::noTrayAvailableAcceptsClose()
{
    auto d = MainController::evaluateCloseRequest(
        /*reallyQuit=*/false, /*closeToTrayPref=*/true,
        /*trayAvail=*/false, /*toastShown=*/false);
    QVERIFY(d.acceptClose);
    QVERIFY(!d.hideWindow);
    QVERIFY(!d.emitToast);
}

void TestMainControllerCloseLogic::hideToTrayFirstTime()
{
    auto d = MainController::evaluateCloseRequest(
        /*reallyQuit=*/false, /*closeToTrayPref=*/true,
        /*trayAvail=*/true, /*toastShown=*/false);
    QVERIFY(!d.acceptClose);
    QVERIFY(d.hideWindow);
    QVERIFY(d.emitToast);
}

void TestMainControllerCloseLogic::hideToTrayDoesNotRepeatToast()
{
    auto d = MainController::evaluateCloseRequest(
        /*reallyQuit=*/false, /*closeToTrayPref=*/true,
        /*trayAvail=*/true, /*toastShown=*/true);
    QVERIFY(!d.acceptClose);
    QVERIFY(d.hideWindow);
    QVERIFY(!d.emitToast);
}

QTEST_GUILESS_MAIN(TestMainControllerCloseLogic)
#include "tst_main_controller_close_logic.moc"
