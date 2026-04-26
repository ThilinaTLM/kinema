// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Placeholder for the Discover home. Replaced by the real
// `DiscoverPage.qml` (sections + Continue Watching) in phase 03.
Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title", "Discover")

    KEmptyState {
        anchors.centerIn: parent
        title: i18nc("@info placeholder", "Discover coming soon")
        explanation: i18nc("@info placeholder",
            "TMDB-powered catalogue strips and Continue Watching land here.")
        iconName: "go-home"
    }
}
