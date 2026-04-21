// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "app/KinemaApplication.h"

#include <KAboutData>
#include <KLocalizedString>

#include <QIcon>
#include <QSystemTrayIcon>

#include "kinema_version.h"

namespace kinema {

KinemaApplication::KinemaApplication(int& argc, char** argv)
    : QApplication(argc, argv)
{
}

void KinemaApplication::configure()
{
    QCoreApplication::setOrganizationDomain(QStringLiteral("tlmtech.dev"));
    QCoreApplication::setOrganizationName(QStringLiteral("TLMTech"));
    QCoreApplication::setApplicationName(QStringLiteral("Kinema"));
    QCoreApplication::setApplicationVersion(QStringLiteral(KINEMA_VERSION_STRING));

    QGuiApplication::setDesktopFileName(QStringLiteral("dev.tlmtech.kinema"));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Kinema"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("dev.tlmtech.kinema")));

    KLocalizedString::setApplicationDomain("kinema");

    KAboutData about(
        QStringLiteral("kinema"),
        i18nc("@title", "Kinema"),
        QStringLiteral(KINEMA_VERSION_STRING),
        i18n("Search movies and TV series, grab magnet links, stream via Real-Debrid."),
        KAboutLicense::GPL_V2,
        i18n("© 2026 Thilina Lakshan"),
        QString{},
        QStringLiteral("https://tlmtech.dev/kinema"));
    about.addAuthor(
        i18nc("@info:credit", "Thilina Lakshan"),
        i18nc("@info:credit", "Author and maintainer"),
        QStringLiteral("thilina@tlmtech.dev"));
    about.setDesktopFileName(QStringLiteral("dev.tlmtech.kinema"));
    about.setProductName(QByteArrayLiteral("kinema"));
    about.setOrganizationDomain(QByteArrayLiteral("tlmtech.dev"));

    KAboutData::setApplicationData(about);

    // When a system tray is available, MainWindow hides to tray on
    // close (see MainWindow::closeEvent) and we want the app to keep
    // running even if every window is hidden. When no tray host is
    // available, the current behaviour — quit when the last window
    // closes — remains correct.
    setQuitOnLastWindowClosed(
        !QSystemTrayIcon::isSystemTrayAvailable());
}

} // namespace kinema
