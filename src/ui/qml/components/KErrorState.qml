// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

// Error placeholder — same layout as KEmptyState but pinned to the
// error icon and accent. Renders a "Try again" action when
// `retryAction` is set.
//
// Usage:
//   KErrorState {
//       anchors.centerIn: parent
//       title: i18n("Couldn’t reach TMDB")
//       explanation: error
//       retryAction: refreshAction
//   }
Kirigami.PlaceholderMessage {
    id: root

    property string title
    property string explanation
    property string iconName: "dialog-error"
    property var retryAction: null

    text: title
    explanation: root.explanation
    icon.name: iconName
    type: Kirigami.PlaceholderMessage.Actionable

    helpfulAction: retryAction
}
