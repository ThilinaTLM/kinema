// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

pragma Singleton

import QtQuick

QtObject {
    readonly property color foreground: Theme.foreground
    readonly property color muted: Theme.disabled
    readonly property color accent: Theme.accent
    readonly property color accentText: Theme.accentText
    readonly property color positive: Theme.positive
    readonly property color negative: Theme.negative

    function _hexByte(value) {
        const clamped = Math.max(0, Math.min(255, Math.round(value * 255)));
        const text = clamped.toString(16);
        return text.length < 2 ? "0" + text : text;
    }

    function hex(color) {
        if (!color) {
            return "#000000";
        }

        const text = String(color);
        if (text.length === 7 && text[0] === "#") {
            return text.toLowerCase();
        }
        if (text.length === 9 && text[0] === "#") {
            return ("#" + text.slice(3)).toLowerCase();
        }

        return "#"
            + _hexByte(color.r)
            + _hexByte(color.g)
            + _hexByte(color.b);
    }

    function url(iconId, color) {
        if (!iconId || iconId.length === 0) {
            return "";
        }
        const tint = color || foreground;
        return AppIconResolver.url(iconId, hex(tint));
    }

    function controlColor(enabled, highlighted) {
        if (!enabled) {
            return muted;
        }
        return highlighted ? accentText : foreground;
    }
}
