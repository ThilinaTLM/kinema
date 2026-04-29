// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// Unified popup-style filter button used by `PageHeaderBar` filter rows.
//
// One widget for two shapes:
//
//   * Pick-one ("Sort: Newest first \u25BE"). Set `multiSelect: false`,
//     bind `currentValue` to the VM's scalar property, and react to
//     `activated(value)` to write back.
//   * Multi-select ("Genres (3) \u25BE"). Set `multiSelect: true`,
//     bind `currentValues` to the VM's list, and react to
//     `multiActivated(values)` to write back the new list. The menu
//     prepends a "Clear" item when at least one option is selected.
//
// `options` is a flat list of `{ value, label }`. Mixing separators is
// not supported on purpose — pages that previously used a separator
// to set a "smart" sort apart are reorganised to render that toggle
// inline instead.
//
// Visuals match the rest of the merged page header: flat `ToolButton`
// with `TextBesideIcon`, checked-while-open behaviour, popup width
// clamps to `gridUnit * 14` minimum.
QQC2.ToolButton {
    id: root

    // ---- API -----------------------------------------------------
    property string axisLabel: ""
    property bool multiSelect: false
    property var options: []           // [{ value, label }]
    property var currentValue           // single-select scalar
    property var currentValues: []      // multi-select list

    signal activated(var value)
    signal multiActivated(var values)

    // ---- Visuals -------------------------------------------------
    flat: true
    display: QQC2.AbstractButton.TextBesideIcon
    checkable: true
    checked: menu.opened

    text: {
        if (root.multiSelect) {
            const n = (root.currentValues || []).length;
            return n > 0
                ? i18nc(
                    "@action:button filter menu, %1 axis name, %2 selection count",
                    "%1 (%2)", root.axisLabel, n)
                : root.axisLabel;
        }
        for (let i = 0; i < (root.options || []).length; ++i) {
            const o = root.options[i];
            if (o && o.value === root.currentValue) {
                return i18nc(
                    "@action:button filter menu, %1 axis name, %2 selected option",
                    "%1: %2", root.axisLabel,
                    o.label !== undefined ? o.label : "");
            }
        }
        return root.axisLabel;
    }

    onClicked: menu.opened ? menu.close() : menu.open()

    // ---- Popup ---------------------------------------------------
    QQC2.Menu {
        id: menu
        y: root.height
        implicitWidth: Math.max(root.width,
            Kirigami.Units.gridUnit * 14)

        // "Clear" affordance at the top of multi-select menus.
        QQC2.MenuItem {
            text: i18nc("@action:inmenu clear multi-select selection",
                "Clear")
            icon.name: "edit-clear-history"
            visible: root.multiSelect
            height: visible ? implicitHeight : 0
            enabled: root.multiSelect
                && (root.currentValues || []).length > 0
            onTriggered: root.multiActivated([])
        }
        QQC2.MenuSeparator {
            visible: root.multiSelect
            height: visible ? implicitHeight : 0
        }

        Repeater {
            model: root.options

            delegate: QQC2.MenuItem {
                required property var modelData
                text: modelData && modelData.label !== undefined
                    ? modelData.label : ""
                checkable: true
                checked: root.multiSelect
                    ? (root.currentValues || []).indexOf(
                        modelData ? modelData.value : undefined) >= 0
                    : (modelData && modelData.value === root.currentValue)
                onTriggered: {
                    if (!modelData) return;
                    if (root.multiSelect) {
                        const next = (root.currentValues || []).slice();
                        const at = next.indexOf(modelData.value);
                        if (at >= 0) {
                            next.splice(at, 1);
                        } else {
                            next.push(modelData.value);
                        }
                        root.multiActivated(next);
                    } else {
                        root.activated(modelData.value);
                        menu.close();
                    }
                }
            }
        }
    }
}
