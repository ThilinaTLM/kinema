// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QApplication>

namespace kinema::config {
class AppSettings;
}

namespace kinema {

/**
 * QApplication subclass that owns process-wide identity (organisation /
 * application name, desktop file id, about data) and the root
 * AppSettings object. Splitting this out of main() keeps main() tiny
 * and avoids any global settings singleton — every collaborator gets
 * AppSettings (or a sub-settings) through a constructor argument.
 */
class KinemaApplication : public QApplication
{
    Q_OBJECT
public:
    KinemaApplication(int& argc, char** argv);
    ~KinemaApplication() override;

    /// Sets QCoreApplication identity and KAboutData. Safe to call once.
    void configure();

    /// Root settings object. Constructed lazily on first access.
    config::AppSettings& settings();

private:
    config::AppSettings* m_settings {};
};

} // namespace kinema
