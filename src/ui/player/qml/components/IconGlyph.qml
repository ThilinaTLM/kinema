// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Shapes
import dev.tlmtech.kinema.player

/**
 * Vector icon glyphs for the player chrome. Every kind is a single
 * `Shape` with one or more `ShapePath { PathSvg }` entries authored
 * against a fixed 24 x 24 logical viewBox. The whole glyph wrapper
 * is scaled to whatever pixel size the host `Item` was given via a
 * uniform `scale:` on a 24 x 24 inner Item — this keeps every
 * glyph's coordinate space identical regardless of host size.
 *
 * Why PathSvg and not pre-baked SVG / icon fonts:
 *
 *   - No runtime asset dependencies — the player chrome's design
 *     contract.
 *   - Theme-coloured strokes for free (`Theme.foreground`,
 *     `Theme.iconStroke`).
 *   - Perfect HiDPI fidelity at any item size.
 *
 * Stroke language: 1.8 px (`Theme.iconStroke`) at the 24 px viewBox,
 * round caps + round joins, monochrome. Filled silhouettes only
 * for play / pause and for the speaker body. Everything else is
 * outline-stroke for consistent visual weight in a row.
 */
Item {
    id: root
    property string kind: ""
    property color color: Theme.foreground
    width: 22
    height: 22

    // Inner 24 x 24 canvas, uniformly scaled to fill the host. All
    // Shapes below anchor to this so their PathSvg coordinates
    // (authored against a 24-unit viewBox) land where they should.
    Item {
        id: vb
        width: 24
        height: 24
        anchors.centerIn: parent
        scale: Math.min(root.width, root.height) / 24
        transformOrigin: Item.Center

        // ---- Play: filled rounded triangle ------------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "play"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                joinStyle: ShapePath.RoundJoin
                capStyle: ShapePath.RoundCap
                PathSvg {
                    path: "M 7 4.6 L 19.2 11.4 Q 20 12 19.2 12.6 L 7 19.4 Q 6 20 6 18.8 L 6 5.2 Q 6 4 7 4.6 Z"
                }
            }
        }

        // ---- Pause: two rounded bars ------------------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "pause"
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg {
                    path: "M 7 4 H 10 Q 11 4 11 5 V 19 Q 11 20 10 20 H 7 Q 6 20 6 19 V 5 Q 6 4 7 4 Z M 14 4 H 17 Q 18 4 18 5 V 19 Q 18 20 17 20 H 14 Q 13 20 13 19 V 5 Q 13 4 14 4 Z"
                }
            }
        }

        // ---- Audio (high volume): speaker + 2 curved waves --------
        Shape {
            anchors.fill: parent
            visible: root.kind === "audio"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg {
                    path: "M 4 9 H 7 L 11 5.5 Q 12 4.7 12 5.8 V 18.2 Q 12 19.3 11 18.5 L 7 15 H 4 Q 3 15 3 14 V 10 Q 3 9 4 9 Z"
                }
            }
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 14.5 9.2 Q 16.2 12 14.5 14.8" }
            }
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 17 7 Q 20 12 17 17" }
            }
        }

        // ---- Volume low: speaker + single short wave --------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "volumeLow"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg {
                    path: "M 4 9 H 7 L 11 5.5 Q 12 4.7 12 5.8 V 18.2 Q 12 19.3 11 18.5 L 7 15 H 4 Q 3 15 3 14 V 10 Q 3 9 4 9 Z"
                }
            }
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 14.5 9.2 Q 16.2 12 14.5 14.8" }
            }
        }

        // ---- Mute: speaker + single diagonal slash ----------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "mute"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg {
                    path: "M 4 9 H 7 L 11 5.5 Q 12 4.7 12 5.8 V 18.2 Q 12 19.3 11 18.5 L 7 15 H 4 Q 3 15 3 14 V 10 Q 3 9 4 9 Z"
                }
            }
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 14 8 L 21 18" }
            }
        }

        // ---- Audio tracks: list lines + tiny musical-note mark.
        //      Distinct from the volume speaker so the two never
        //      look like duplicate icons in the row. -----------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "audioTracks"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 4 7 H 14" }
                PathSvg { path: "M 4 12 H 11" }
                PathSvg { path: "M 4 17 H 14" }
                // Note stem on the right
                PathSvg { path: "M 18 6 V 14.5" }
                // Beam from stem-top to a small flag
                PathSvg { path: "M 18 6 L 21 7.5" }
            }
            // Filled note head
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg {
                    path: "M 18 14.5 m -1.6 0 a 1.6 1.6 0 1 0 3.2 0 a 1.6 1.6 0 1 0 -3.2 0 Z"
                }
            }
        }

        // ---- Subtitle: rounded rect + two interior caption lines
        Shape {
            anchors.fill: parent
            visible: root.kind === "subtitle"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                joinStyle: ShapePath.RoundJoin
                capStyle: ShapePath.RoundCap
                PathSvg {
                    path: "M 4 6 H 20 Q 21 6 21 7 V 17 Q 21 18 20 18 H 4 Q 3 18 3 17 V 7 Q 3 6 4 6 Z"
                }
                PathSvg { path: "M 6 11 H 10" }
                PathSvg { path: "M 12 11 H 18" }
                PathSvg { path: "M 6 14.5 H 14" }
                PathSvg { path: "M 16 14.5 H 18" }
            }
        }

        // ---- Speed: half-circle gauge + needle --------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "speed"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 4 16 A 8 8 0 0 1 20 16" }
                PathSvg { path: "M 6.5 11 L 7.4 11.7" }
                PathSvg { path: "M 12 8 V 9.2" }
                PathSvg { path: "M 17.5 11 L 16.6 11.7" }
                PathSvg { path: "M 12 16 L 16 11.5" }
            }
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg {
                    path: "M 12 16 m -1.4 0 a 1.4 1.4 0 1 0 2.8 0 a 1.4 1.4 0 1 0 -2.8 0 Z"
                }
            }
        }

        // ---- Info: circled lowercase i ----------------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "info"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 12 4 m -8 0 a 8 8 0 1 0 16 0 a 8 8 0 1 0 -16 0 Z" }
                PathSvg { path: "M 12 11 V 17" }
            }
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg { path: "M 12 7.5 m -1 0 a 1 1 0 1 0 2 0 a 1 1 0 1 0 -2 0 Z" }
            }
        }

        // ---- Fullscreen: four outward corner arrows ---------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "fullscreen"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M 9 4 H 4 V 9" }
                PathSvg { path: "M 15 4 H 20 V 9" }
                PathSvg { path: "M 20 15 V 20 H 15" }
                PathSvg { path: "M 4 15 V 20 H 9" }
            }
        }

        // ---- Exit fullscreen: four inward corner arrows -----------
        Shape {
            anchors.fill: parent
            visible: root.kind === "exitFullscreen"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M 4 9 H 9 V 4" }
                PathSvg { path: "M 15 4 V 9 H 20" }
                PathSvg { path: "M 20 15 H 15 V 20" }
                PathSvg { path: "M 9 20 V 15 H 4" }
            }
        }

        // ---- Close pill cross -------------------------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "x"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg { path: "M 6 6 L 18 18" }
                PathSvg { path: "M 18 6 L 6 18" }
            }
        }
    }
}
