// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Idle / "do something to begin" placeholder. Thin wrapper around
// Kirigami.PlaceholderMessage so callers don't have to remember
// the icon-name conventions and positional anchoring boilerplate.
//
// Usage:
//   KEmptyState {
//       anchors.centerIn: parent
//       title: i18n("Search to get started")
//       explanation: i18n("Type a movie title or IMDB id and press Enter.")
//       iconName: "search"          // optional, defaults to "documentinfo"
//   }
Kirigami.PlaceholderMessage {
    id: root

    property string title
    property string explanation
    property string iconName: "documentinfo"

    text: title
    explanation: root.explanation
    icon.name: iconName
}
