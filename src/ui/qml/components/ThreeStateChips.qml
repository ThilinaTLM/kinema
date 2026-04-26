// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Three-state segmented control. Maps to `controllers::SubtitleController`'s
// "off" / "include" / "only" filter modes for hearing-impaired and
// foreign-parts-only subtitles. Used inside a FormCard delegate so
// it inherits the right padding from `AbstractFormDelegate`.
RowLayout {
    id: chips
    spacing: Kirigami.Units.smallSpacing

    /// Localized label for the row ("Hearing impaired", "Foreign parts only").
    property string label

    /// Current value — one of "off", "include", "only".
    property string value: "off"

    /// Renamed from `valueChanged` to avoid clashing with the
    /// implicit `value` property-change signal.
    signal valuePicked(string newValue)

    QQC2.Label {
        text: chips.label
        Layout.fillWidth: true
        elide: Text.ElideRight
    }

    QQC2.ButtonGroup { id: group }

    Repeater {
        model: [
            { id: "off",     label: i18nc("@option:radio subtitle filter mode", "Off") },
            { id: "include", label: i18nc("@option:radio subtitle filter mode", "Include") },
            { id: "only",    label: i18nc("@option:radio subtitle filter mode", "Only") }
        ]
        delegate: QQC2.Button {
            required property var modelData
            text: modelData.label
            checkable: true
            checked: chips.value === modelData.id
            QQC2.ButtonGroup.group: group
            onClicked: {
                if (chips.value !== modelData.id) {
                    chips.valuePicked(modelData.id);
                }
            }
        }
    }
}
