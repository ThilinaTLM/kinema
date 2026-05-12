// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Tiny label pill used by `BackdropHero` and the list cards for genre /
// codec / source / quality tags. Rendered with a 1 px outline rather
// than a filled background so multiple chips next to each other don't
// turn into a colour wall over the backdrop.
//
// `tone` accepts: "neutral" (default), "accent", "positive", "negative".
// Tones map to Kirigami palette colours so the chip follows the user's
// Plasma theme without us picking literal hex values.
QQC2.Control {
    id: chip

    property string text
    property string tone: "neutral"
    // Optional leading icon. Empty by default — call sites that
    // want a plain text pill ignore it. When set, the chip renders
    // icon + label inside the same border so multiple icon-bearing
    // chips line up with text-only chips on the same rail.
    // `var` so this accepts both `url` (image://, file:// from
    // AppIcons.url) and bare XDG icon names (e.g. "folder-cloud").
    property var iconSource
    property color iconColor: chip.labelColor

    leftPadding: Kirigami.Units.smallSpacing * 2
    rightPadding: Kirigami.Units.smallSpacing * 2
    topPadding: Math.round(Kirigami.Units.smallSpacing / 2)
    bottomPadding: Math.round(Kirigami.Units.smallSpacing / 2)

    readonly property color borderColor: tone === "accent"
        ? Theme.accent
        : (tone === "positive"
            ? Theme.positive
            : (tone === "negative"
                ? Theme.negative
                : Qt.alpha(Theme.foreground, 0.30)))

    readonly property color labelColor: tone === "accent"
        ? Theme.accent
        : (tone === "positive"
            ? Theme.positive
            : (tone === "negative"
                ? Theme.negative
                : Theme.foreground))

    background: Rectangle {
        radius: chip.height / 2
        color: "transparent"
        border.color: chip.borderColor
        border.width: 1
    }

    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing

        Kirigami.Icon {
            visible: chip.iconSource !== undefined
                && ("" + chip.iconSource).length > 0
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: width
            source: chip.iconSource
            color: chip.iconColor
        }

        QQC2.Label {
            Layout.fillWidth: true
            text: chip.text
            color: chip.labelColor
            font.pointSize: Theme.captionFont.pointSize
            font.weight: Font.Medium
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
}
