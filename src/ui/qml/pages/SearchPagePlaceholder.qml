// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Placeholder for the Search page. Replaced by the real
// `SearchPage.qml` (Kirigami.SearchField in header + grid) in
// phase 04.
Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title", "Search")

    KEmptyState {
        anchors.centerIn: parent
        title: i18nc("@info placeholder", "Search coming soon")
        explanation: i18nc("@info placeholder",
            "Type a movie title or IMDB id to find releases.")
        iconName: "search"
    }
}
