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
 * The chrome icons are transcoded directly from the Lucide icon set
 * (`audio-lines`, `captions`, `gauge`, `maximize`, `minimize`,
 * `pause`, `play`, `volume`, `volume-1`, `volume-2`, `volume-off`,
 * `volume-x`). Each Lucide source file is one or more `<path d>`
 * entries with `stroke="currentColor"`, `stroke-width="2"`, round
 * caps + joins. We render them with `Theme.iconStroke` and the host
 * `color` property so the glyph adopts the surrounding theme.
 *
 * Why PathSvg and not pre-baked SVG / icon fonts:
 *
 *   - No runtime asset dependencies — the player chrome's design
 *     contract.
 *   - Theme-coloured strokes for free (`Theme.foreground`,
 *     `Theme.iconStroke`).
 *   - Perfect HiDPI fidelity at any item size.
 *
 * Stroke language: `Theme.iconStroke` at the 24 px viewBox, round
 * caps + round joins, monochrome outlines. Filled silhouettes only
 * exist for the `playSolid` / `pauseSolid` kinds, which the
 * transient `CenterFlash` overlay uses — a thin outline reads weak
 * as a 96 px pulse, so the flash keeps the bold filled look while
 * the chrome itself stays in the outline idiom.
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

        // ---- Play (Lucide outline triangle) ----------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "play"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg {
                    path: "M 5 5 a 2 2 0 0 1 3.008 -1.728 l 11.997 6.998 a 2 2 0 0 1 0.003 3.458 l -12 7 A 2 2 0 0 1 5 19 Z"
                }
            }
        }

        // ---- Pause (Lucide two outline rounded rects) ------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "pause"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                // x=14 y=3 w=5 h=18 rx=1
                PathSvg {
                    path: "M 15 3 H 18 A 1 1 0 0 1 19 4 V 20 A 1 1 0 0 1 18 21 H 15 A 1 1 0 0 1 14 20 V 4 A 1 1 0 0 1 15 3 Z"
                }
                // x=5 y=3 w=5 h=18 rx=1
                PathSvg {
                    path: "M 6 3 H 9 A 1 1 0 0 1 10 4 V 20 A 1 1 0 0 1 9 21 H 6 A 1 1 0 0 1 5 20 V 4 A 1 1 0 0 1 6 3 Z"
                }
            }
        }

        // ---- Play (filled silhouette, used by CenterFlash) -------
        Shape {
            anchors.fill: parent
            visible: root.kind === "playSolid"
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

        // ---- Pause (filled silhouette, used by CenterFlash) ------
        Shape {
            anchors.fill: parent
            visible: root.kind === "pauseSolid"
            ShapePath {
                strokeWidth: 0
                fillColor: root.color
                PathSvg {
                    path: "M 7 4 H 10 Q 11 4 11 5 V 19 Q 11 20 10 20 H 7 Q 6 20 6 19 V 5 Q 6 4 7 4 Z M 14 4 H 17 Q 18 4 18 5 V 19 Q 18 20 17 20 H 14 Q 13 20 13 19 V 5 Q 13 4 14 4 Z"
                }
            }
        }

        // ---- Volume (Lucide silent speaker) ----------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "volume"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg {
                    path: "M 11 4.702 a 0.705 0.705 0 0 0 -1.203 -0.498 L 6.413 7.587 A 1.4 1.4 0 0 1 5.416 8 H 3 a 1 1 0 0 0 -1 1 v 6 a 1 1 0 0 0 1 1 h 2.416 a 1.4 1.4 0 0 1 0.997 0.413 l 3.383 3.384 A 0.705 0.705 0 0 0 11 19.298 Z"
                }
            }
        }

        // ---- Volume-1 (Lucide speaker + 1 short wave) ------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "volume1"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg {
                    path: "M 11 4.702 a 0.705 0.705 0 0 0 -1.203 -0.498 L 6.413 7.587 A 1.4 1.4 0 0 1 5.416 8 H 3 a 1 1 0 0 0 -1 1 v 6 a 1 1 0 0 0 1 1 h 2.416 a 1.4 1.4 0 0 1 0.997 0.413 l 3.383 3.384 A 0.705 0.705 0 0 0 11 19.298 Z"
                }
                PathSvg { path: "M 16 9 a 5 5 0 0 1 0 6" }
            }
        }

        // ---- Volume-2 (Lucide speaker + 2 waves) -----------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "volume2"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg {
                    path: "M 11 4.702 a 0.705 0.705 0 0 0 -1.203 -0.498 L 6.413 7.587 A 1.4 1.4 0 0 1 5.416 8 H 3 a 1 1 0 0 0 -1 1 v 6 a 1 1 0 0 0 1 1 h 2.416 a 1.4 1.4 0 0 1 0.997 0.413 l 3.383 3.384 A 0.705 0.705 0 0 0 11 19.298 Z"
                }
                PathSvg { path: "M 16 9 a 5 5 0 0 1 0 6" }
                PathSvg { path: "M 19.364 18.364 a 9 9 0 0 0 0 -12.728" }
            }
        }

        // ---- Volume-off (Lucide slashed speaker, partial waves) --
        Shape {
            anchors.fill: parent
            visible: root.kind === "volumeOff"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M 16 9 a 5 5 0 0 1 0.95 2.293" }
                PathSvg { path: "M 19.364 5.636 a 9 9 0 0 1 1.889 9.96" }
                PathSvg { path: "M 2 2 L 22 22" }
                PathSvg {
                    path: "M 7 7 L 6.413 7.587 A 1.4 1.4 0 0 1 5.416 8 H 3 a 1 1 0 0 0 -1 1 v 6 a 1 1 0 0 0 1 1 h 2.416 a 1.4 1.4 0 0 1 0.997 0.413 l 3.383 3.384 A 0.705 0.705 0 0 0 11 19.298 V 11"
                }
                PathSvg {
                    path: "M 9.828 4.172 A 0.686 0.686 0 0 1 11 4.657 V 5.343"
                }
            }
        }

        // ---- Volume-x (Lucide speaker + X) -----------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "volumeX"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg {
                    path: "M 11 4.702 a 0.705 0.705 0 0 0 -1.203 -0.498 L 6.413 7.587 A 1.4 1.4 0 0 1 5.416 8 H 3 a 1 1 0 0 0 -1 1 v 6 a 1 1 0 0 0 1 1 h 2.416 a 1.4 1.4 0 0 1 0.997 0.413 l 3.383 3.384 A 0.705 0.705 0 0 0 11 19.298 Z"
                }
                PathSvg { path: "M 22 9 L 16 15" }
                PathSvg { path: "M 16 9 L 22 15" }
            }
        }

        // ---- Audio-lines (Lucide six vertical bars) --------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "audioLines"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M 2 10 V 13" }
                PathSvg { path: "M 6 6 V 17" }
                PathSvg { path: "M 10 3 V 21" }
                PathSvg { path: "M 14 8 V 15" }
                PathSvg { path: "M 18 5 V 18" }
                PathSvg { path: "M 22 10 V 13" }
            }
        }

        // ---- Captions (Lucide rounded rect + caption strokes) ----
        Shape {
            anchors.fill: parent
            visible: root.kind === "captions"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                // <rect x=3 y=5 w=18 h=14 rx=2 ry=2>
                PathSvg {
                    path: "M 5 5 H 19 A 2 2 0 0 1 21 7 V 17 A 2 2 0 0 1 19 19 H 5 A 2 2 0 0 1 3 17 V 7 A 2 2 0 0 1 5 5 Z"
                }
                PathSvg { path: "M 7 15 H 11" }
                PathSvg { path: "M 15 15 H 17" }
                PathSvg { path: "M 7 11 H 9" }
                PathSvg { path: "M 13 11 H 17" }
            }
        }

        // ---- Gauge (Lucide arc + needle) -------------------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "gauge"
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M 12 14 L 16 10" }
                PathSvg { path: "M 3.34 19 a 10 10 0 1 1 17.32 0" }
            }
        }

        // ---- Maximize (Lucide four corner arrows) ----------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "maximize"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M 8 3 H 5 A 2 2 0 0 0 3 5 V 8" }
                PathSvg { path: "M 21 8 V 5 A 2 2 0 0 0 19 3 H 16" }
                PathSvg { path: "M 3 16 V 19 A 2 2 0 0 0 5 21 H 8" }
                PathSvg { path: "M 16 21 H 19 A 2 2 0 0 0 21 19 V 16" }
            }
        }

        // ---- Minimize (Lucide four inward arrows) ----------------
        Shape {
            anchors.fill: parent
            visible: root.kind === "minimize"
            ShapePath {
                strokeColor: root.color
                strokeWidth: Theme.iconStroke
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M 8 3 V 6 A 2 2 0 0 1 6 8 H 3" }
                PathSvg { path: "M 21 8 H 18 A 2 2 0 0 1 16 6 V 3" }
                PathSvg { path: "M 3 16 H 6 A 2 2 0 0 1 8 18 V 21" }
                PathSvg { path: "M 16 21 V 18 A 2 2 0 0 1 18 16 H 21" }
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
