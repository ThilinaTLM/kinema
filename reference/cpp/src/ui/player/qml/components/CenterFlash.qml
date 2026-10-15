// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import dev.tlmtech.kinema.player

/**
 * Transient centre glyph that flashes on pause / unpause.
 * `flash(kind)` triggers a quick scale + fade animation.
 */
Item {
    id: root
    property string kind: ""
    width: 96
    height: 96

    function flash(k) {
        kind = k;
        bg.scale = 0.6;
        bg.opacity = 0.95;
        anim.restart();
    }

    SequentialAnimation {
        id: anim
        ParallelAnimation {
            NumberAnimation {
                target: bg; property: "scale"
                to: 1.0; duration: 220
                easing.type: Easing.OutBack
            }
        }
        NumberAnimation {
            target: bg; property: "opacity"
            to: 0.0; duration: 320
            easing.type: Easing.InQuad
        }
    }

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: width / 2
        color: Qt.rgba(0, 0, 0, 0.55)
        opacity: 0.0
        scale: 0.6

        IconGlyph {
            anchors.centerIn: parent
            // The chrome's play/pause are outline strokes, which
            // read weak as a 96 px transient pulse. The flash uses
            // the bold filled silhouettes (`playSolid`/`pauseSolid`)
            // dedicated to it.
            kind: root.kind === "play"
                ? "playSolid"
                : root.kind === "pause" ? "pauseSolid" : root.kind
            width: 48
            height: 48
            color: Theme.foreground
        }
    }
}
