// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

// Thin wrapper around `Kirigami.AboutPage`. The Kirigami page
// accepts either a JS-shaped object or a native `KAboutData`
// instance; we feed it the real one through `mainController` so
// authors / licenses / version stay in sync with `KAboutData::
// applicationData()` populated in `KinemaApplication::configure()`.
Kirigami.AboutPage {
    title: i18nc("@title:window", "About Kinema")
    aboutData: mainController.aboutData
}
