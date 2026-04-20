// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "app/KinemaApplication.h"
#include "ui/MainWindow.h"

#include <KAboutData>

#include <QCommandLineParser>

int main(int argc, char* argv[])
{
    kinema::KinemaApplication app(argc, argv);
    app.configure();

    // --help / --version, plus room for future CLI flags.
    QCommandLineParser parser;
    KAboutData::applicationData().setupCommandLine(&parser);
    parser.process(app);
    KAboutData::applicationData().processCommandLine(&parser);

    kinema::ui::MainWindow window;
    window.show();

    return app.exec();
}
