// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

// Multi-select language picker for the subtitles flow. Drops in
// inside a FormCard.FormCard as a single delegate; the popup is a
// menu of checkable common-language entries pulled from the VM's
// `commonLanguages` property. Mirrors the public API of the deleted
// `LanguagePickerButton` widget: a single `languages` string list
// in / out, plus a "no languages" placeholder.
FormCard.AbstractFormDelegate {
    id: root

    /// Currently selected ISO 639-2 codes.
    property var languages: []
    /// `commonLanguages` array from the VM ([{ code, display }, ...]).
    property var options: []

    /// Emitted when the user picks a code in the menu. Renamed away
    /// from `languagesChanged` to avoid clashing with the implicit
    /// `languages` property-change signal.
    signal languagesPicked(var newCodes)

    background: null
    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            QQC2.Label {
                text: i18nc("@label:listbox subtitles language picker",
                    "Languages")
                Layout.fillWidth: true
            }
            QQC2.Label {
                text: root.languages.length === 0
                    ? i18nc("@info language picker, no selection",
                        "Any language")
                    : root.languages.length <= 3
                        ? root.languages.map(c => c.toUpperCase())
                            .join(", ")
                        : i18ncp("@info language picker, count summary",
                            "%1 language", "%1 languages",
                            root.languages.length)
                Layout.fillWidth: true
                opacity: 0.7
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                elide: Text.ElideRight
            }
        }

        QQC2.ToolButton {
            text: i18nc("@action:button language picker, open menu",
                "Choose…")
            icon.name: "preferences-desktop-locale"
            onClicked: menu.open()

            QQC2.Menu {
                id: menu
                Repeater {
                    model: root.options
                    delegate: QQC2.MenuItem {
                        required property var modelData
                        text: `${modelData.display} (${modelData.code.toUpperCase()})`
                        checkable: true
                        checked: root.languages.indexOf(modelData.code) >= 0
                        onTriggered: {
                            const code = modelData.code;
                            const next = root.languages.slice();
                            const idx = next.indexOf(code);
                            if (idx >= 0) {
                                next.splice(idx, 1);
                            } else {
                                next.push(code);
                            }
                            root.languagesPicked(next);
                        }
                    }
                }
                QQC2.MenuSeparator {}
                QQC2.MenuItem {
                    text: i18nc("@action language picker, drop selection",
                        "Clear (any language)")
                    enabled: root.languages.length > 0
                    onTriggered: root.languagesPicked([])
                }
            }
        }
    }
}
