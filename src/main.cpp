// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "app/KinemaApplication.h"
#include "ui/MainWindow.h"

#include <KAboutData>

#include <QCommandLineParser>

#ifdef KINEMA_HAVE_LIBMPV
#include <clocale>
#endif

int main(int argc, char* argv[])
{
    kinema::KinemaApplication app(argc, argv);
    app.configure();

    // --help / --version, plus room for future CLI flags.
    QCommandLineParser parser;
    KAboutData::applicationData().setupCommandLine(&parser);
    parser.process(app);
    KAboutData::applicationData().processCommandLine(&parser);

#ifdef KINEMA_HAVE_LIBMPV
    // libmpv requires LC_NUMERIC to be "C" — otherwise its config
    // parser flips decimal separators in locales like de_DE and
    // mpv_create() aborts with "Non-C locale detected". QApplication
    // inherits the user's locale on construction, so we force
    // LC_NUMERIC back here, after QApplication has done its own
    // locale handling but before any MpvWidget is created. Only
    // LC_NUMERIC is touched, so Qt's locale-aware formatting and
    // the rest of the C library are unaffected.
    std::setlocale(LC_NUMERIC, "C");
#endif

    kinema::ui::MainWindow window;
    window.show();

    return app.exec();
}
