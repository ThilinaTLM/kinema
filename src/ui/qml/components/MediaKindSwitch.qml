// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Two-segment Movies/Series toggle. `kind` is an int that mirrors
// `api::MediaKind` (0 = Movie, 1 = Series) so the consuming
// view-model property can bind to it directly. Emits `activated`
// with the new value when the user picks a button; the binding
// itself stays write-once-from-the-VM to avoid dueling updates.
//
// Used by the Search page header today; the Browse drawer reuses
// the same component.
RowLayout {
    id: root

    property int kind: 0
    signal activated(int newKind)

    spacing: 0

    QQC2.ButtonGroup { id: group }

    QQC2.Button {
        Layout.fillWidth: false
        text: i18nc("@option:radio media kind", "Movies")
        icon.name: "video-x-generic"
        QQC2.ButtonGroup.group: group
        checkable: true
        checked: root.kind === 0
        onClicked: if (root.kind !== 0) root.activated(0)
    }
    QQC2.Button {
        Layout.fillWidth: false
        text: i18nc("@option:radio media kind", "TV Series")
        icon.name: "view-media-recent"
        QQC2.ButtonGroup.group: group
        checkable: true
        checked: root.kind === 1
        onClicked: if (root.kind !== 1) root.activated(1)
    }
}
