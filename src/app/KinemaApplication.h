// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QApplication>

namespace kinema {

/**
 * QApplication subclass that owns process-wide identity (organisation /
 * application name, desktop file id, about data). Splitting this out of
 * main() keeps main() tiny and makes the setup testable if we ever need to.
 */
class KinemaApplication : public QApplication
{
    Q_OBJECT
public:
    KinemaApplication(int& argc, char** argv);

    /// Sets QCoreApplication identity and KAboutData. Safe to call once.
    void configure();
};

} // namespace kinema
