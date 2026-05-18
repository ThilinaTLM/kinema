// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2

import dev.tlmtech.kinema.app

// Standard menu item for every QQC2.Menu / KinemaMenu in the app.
// Wraps QQC2.MenuItem and applies the visual contract documented in
// docs/MenuConventions.md:
//
//   - `iconName`     : Lucide id resolved via AppIcons.url(...).
//                      Empty string leaves the item icon-less.
//   - `destructive`  : tints the icon with AppIcons.negative; place
//                      these items at the bottom of the menu, after
//                      the last MenuSeparator.
//
// Consumers set `label` instead of `text`. `text` is bound from
// `label` and should not be overridden.
//
// Labels are short, title-case, verb-first; never use a trailing
// ellipsis. Items that open a confirm dialog wire the dialog from
// `onTriggered` directly \u2014 the label stays the same as if the
// action ran immediately.
QQC2.MenuItem {
    id: root

    property string iconName: ""
    property bool destructive: false

    // Source label coming from i18nc(). Kept as a separate prop so
    // future styling tweaks (icon-only variants, badges, \u2026) have
    // somewhere to land without overloading `text`.
    property string label: ""

    text: label

    icon.source: iconName.length > 0 ? AppIcons.url(iconName) : ""
    icon.color: destructive
        ? AppIcons.negative
        : AppIcons.controlColor(enabled, false)
}
