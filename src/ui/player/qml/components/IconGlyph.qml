// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Shapes
import dev.tlmtech.kinema.player

/**
 * Vector icon glyphs for the transport bar. Each `kind` resolves to
 * a small shape drawn with QtQuick Shapes / Rectangles. Keeping
 * icons in code (instead of a font or SVG resources) means zero
 * runtime asset dependencies, perfect HiDPI fidelity, and
 * theme-coloured strokes for free.
 */
Item {
    id: root
    property string kind: ""
    property color color: Theme.foreground
    width: 22
    height: 22

    // Play: filled triangle
    Shape {
        anchors.fill: parent
        visible: root.kind === "play"
        ShapePath {
            strokeWidth: 0
            fillColor: root.color
            startX: 5; startY: 3
            PathLine { x: 19; y: 11 }
            PathLine { x: 5;  y: 19 }
            PathLine { x: 5;  y: 3 }
        }
    }

    // Pause: two rounded rectangles
    Item {
        anchors.fill: parent
        visible: root.kind === "pause"
        Rectangle {
            x: 5; y: 4; width: 4; height: 14; radius: 1.5
            color: root.color
        }
        Rectangle {
            x: 13; y: 4; width: 4; height: 14; radius: 1.5
            color: root.color
        }
    }

    // Speed: stylised "1x" / arrow
    Item {
        anchors.fill: parent
        visible: root.kind === "speed"
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: root.color
                strokeWidth: 2
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                startX: 4; startY: 11
                PathLine { x: 14; y: 11 }
                PathLine { x: 11; y: 8 }
                PathMove { x: 14; y: 11 }
                PathLine { x: 11; y: 14 }
            }
        }
    }

    // Audio: speaker glyph
    Item {
        anchors.fill: parent
        visible: root.kind === "audio"
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                startX: 4;  startY: 9
                PathLine { x: 8;  y: 9 }
                PathLine { x: 13; y: 4 }
                PathLine { x: 13; y: 18 }
                PathLine { x: 8;  y: 13 }
                PathLine { x: 4;  y: 13 }
                PathLine { x: 4;  y: 9 }
            }
            ShapePath {
                strokeColor: root.color
                strokeWidth: 1.6
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                startX: 15; startY: 8
                PathQuad { x: 15; y: 14; controlX: 18; controlY: 11 }
            }
        }
    }

    // Subtitle: "CC" pill
    Rectangle {
        anchors.fill: parent
        visible: root.kind === "subtitle"
        color: "transparent"
        border.color: root.color
        border.width: 1.6
        radius: 4
        Text {
            anchors.centerIn: parent
            text: "CC"
            color: root.color
            font.pixelSize: 11
            font.weight: Font.Bold
        }
    }

    // Info: lowercase "i"
    Item {
        anchors.fill: parent
        visible: root.kind === "info"
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 3; width: 3; height: 3; radius: 1.5
            color: root.color
        }
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 8; width: 3; height: 11; radius: 1
            color: root.color
        }
    }

    // Fullscreen: four corner brackets
    Item {
        anchors.fill: parent
        visible: root.kind === "fullscreen"
        property color c: root.color
        Rectangle { x: 3;  y: 3;  width: 6; height: 2; color: parent.c }
        Rectangle { x: 3;  y: 3;  width: 2; height: 6; color: parent.c }
        Rectangle { x: 13; y: 3;  width: 6; height: 2; color: parent.c }
        Rectangle { x: 17; y: 3;  width: 2; height: 6; color: parent.c }
        Rectangle { x: 3;  y: 17; width: 6; height: 2; color: parent.c }
        Rectangle { x: 3;  y: 13; width: 2; height: 6; color: parent.c }
        Rectangle { x: 13; y: 17; width: 6; height: 2; color: parent.c }
        Rectangle { x: 17; y: 13; width: 2; height: 6; color: parent.c }
    }

    // Mute: speaker with X
    Item {
        anchors.fill: parent
        visible: root.kind === "mute"
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                startX: 4;  startY: 9
                PathLine { x: 8;  y: 9 }
                PathLine { x: 13; y: 4 }
                PathLine { x: 13; y: 18 }
                PathLine { x: 8;  y: 13 }
                PathLine { x: 4;  y: 13 }
                PathLine { x: 4;  y: 9 }
            }
        }
        Rectangle {
            x: 15; y: 9; width: 6; height: 2; color: root.color
            rotation: 45
            transformOrigin: Item.Center
        }
        Rectangle {
            x: 15; y: 9; width: 6; height: 2; color: root.color
            rotation: -45
            transformOrigin: Item.Center
        }
    }
}
