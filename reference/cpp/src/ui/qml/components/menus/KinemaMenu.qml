// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// Thin wrapper around QQC2.Menu that enforces the app's context-menu
// visual contract:
//   - Consistent minimum width so single-icon menus don't shrink to a
//     postage stamp.
//   - View-role theming so the popup sits cleanly above the underlying
//     surface regardless of where it's spawned.
//
// Populate via standard children (KinemaMenuItem, QQC2.MenuSeparator,
// QQC2.Menu sub-menus). See docs/MenuConventions.md for the rules
// every menu in the app follows.
QQC2.Menu {
    id: root

    implicitWidth: Kirigami.Units.gridUnit * 12

    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    modal: true
}
