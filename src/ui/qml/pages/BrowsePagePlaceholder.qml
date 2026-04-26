// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Placeholder for the Browse page. Replaced by the real
// `BrowsePage.qml` (filter overlay drawer + grid) in phase 04.
Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title", "Browse")

    KEmptyState {
        anchors.centerIn: parent
        title: i18nc("@info placeholder", "Browse coming soon")
        explanation: i18nc("@info placeholder",
            "TMDB /discover with filters lands here.")
        iconName: "view-list-icons"
    }
}
