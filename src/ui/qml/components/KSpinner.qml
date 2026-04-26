// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

// Loading state with a centred busy indicator + caption. Wraps
// Kirigami.LoadingPlaceholder so call sites read intent-first.
//
// Usage:
//   KSpinner {
//       anchors.centerIn: parent
//       title: i18n("Loading…")
//   }
Kirigami.LoadingPlaceholder {
    id: root

    property string title

    text: title
}
