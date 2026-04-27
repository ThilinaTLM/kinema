// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Segmented Movies / TV Series toggle.
//
// Visual vocabulary follows the Breeze `QQC2.Button` background
// (`/usr/lib/qt6/qml/org/kde/breeze/impl/ButtonBackground.qml`)
// so the control reads as two Breeze buttons fused into a
// segmented group:
//
//   * `Kirigami.Theme.Button` colour set with `inherit: false`,
//     so the segments use Breeze's button palette regardless of
//     where they're embedded.
//   * Corner radius = `Kirigami.Units.cornerRadius` (Breeze's
//     `Impl.Units.smallRadius`). Outer corners are rounded, inner
//     corners are squared via `Kirigami.ShadowedRectangle.corners`
//     — the same primitive Kirigami-Addons' `SegmentedButton`
//     uses for the same job.
//   * Fill: `backgroundColor` idle, `alternateBackgroundColor`
//     when checked. Border: blended separator colour idle,
//     `focusColor` when checked / hovered / focused. 1 px border
//     width to match Breeze's `smallBorder`.
//   * `spacing: -1` on the row so adjacent borders overlap into a
//     single seam, exactly how Breeze native segmented buttons
//     behave when their parent layout has `spacing <= 0`.
//
// We keep our own delegate (rather than dropping in
// `QQC2.Button`) for two reasons:
//   1. `QQC2.Button`'s segmented-corner trick only triggers under
//      the Breeze QQC2 style; users on Fusion/Basic would get
//      four rounded buttons touching. Doing it ourselves keeps
//      the look correct on every style.
//   2. We want the icon and label colour to track `checked`
//      consistently, including the bold-on-checked treatment
//      already used elsewhere in the app.
//
// Public API (kept stable for `SearchPage` and `BrowseFilterBar`):
//
//   property int kind              // 0 = Movie, 1 = Series
//   signal activated(int newKind)  // emitted on user change only
Item {
    id: root

    property int kind: 0
    signal activated(int newKind)

    // Adopt Breeze's button palette so our colours match the
    // surrounding `QQC2.Button`s in toolbars without us having to
    // hard-code anything.
    Kirigami.Theme.colorSet: Kirigami.Theme.Button
    Kirigami.Theme.inherit: false

    // Sizing strategy: match Breeze's *standard* control height
    // so we line up with every other interactive control in the
    // toolbar (`QQC2.Button`, `ToolButton`, `ComboBox`,
    // `Kirigami.SearchField` — all of them use
    // `padding: Kirigami.Units.largeSpacing` in Breeze, giving
    // an implicit height of `fontMetrics.height + largeSpacing * 2`).
    //
    // We get this for free by using the same `largeSpacing`
    // vertical padding on the segment delegate (see below).
    // `row.implicitHeight` then reports that exact value, so
    // our intrinsic height equals a Breeze button's intrinsic
    // height, and we sit flush next to a `SearchField` /
    // `ComboBox` / `ToolButton` without `gridUnit * 2` overshoot
    // or `smallSpacing` undershoot.
    //
    // No `Layout.fillHeight` is needed — if we already match the
    // tallest sibling, stretching is a no-op; if the toolbar
    // ever ends up with an even taller sibling, the segment's
    // own `Layout.fillHeight: true` will take care of growth.
    implicitHeight: row.implicitHeight
    implicitWidth: row.implicitWidth

    QQC2.ButtonGroup {
        id: group
        exclusive: true
    }

    RowLayout {
        id: row
        anchors.fill: parent
        // Overlap borders into a single 1 px seam — same trick
        // Kirigami-Addons' `SegmentedButton` uses.
        spacing: -1
        // Equal-width segments so different label lengths don't
        // make one side dominate.
        uniformCellSizes: true

        Repeater {
            id: segments
            model: [
                { text: i18nc("@option:radio media kind", "Movies"),
                  iconName: "video-x-generic" },
                { text: i18nc("@option:radio media kind", "TV Series"),
                  iconName: "view-media-recent" }
            ]

            delegate: QQC2.AbstractButton {
                id: segment
                required property var modelData
                required property int index

                readonly property bool isFirst: index === 0
                readonly property bool isLast: index === segments.count - 1
                readonly property bool highlighted:
                    checked || hovered || visualFocus

                Layout.fillWidth: true
                Layout.fillHeight: true

                // Match Breeze's standard control padding
                // (`Kirigami.Units.largeSpacing`) so the segment's
                // implicit height equals a Breeze `QQC2.Button` /
                // `ToolButton` / `ComboBox` / `TextField` — they
                // all use this same value. This is what makes us
                // sit flush next to a `Kirigami.SearchField` in
                // SearchPage and next to `ComboBox` / `ToolButton`
                // in BrowsePage without picking a magic number.
                padding: Kirigami.Units.largeSpacing

                // Raise the highlighted segment so its focus border
                // isn't clipped by the neighbour.
                z: highlighted ? 1 : 0

                checkable: true
                checked: root.kind === index
                hoverEnabled: true
                QQC2.ButtonGroup.group: group

                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData.text
                Accessible.checked: checked

                onClicked: if (root.kind !== index) {
                    root.activated(index);
                }

                background: Kirigami.ShadowedRectangle {
                    // Outer corners rounded, inner corners squared
                    // — the segmented-button corner rule.
                    corners {
                        topLeftRadius:
                            segment.isFirst ? Kirigami.Units.cornerRadius : 0
                        bottomLeftRadius:
                            segment.isFirst ? Kirigami.Units.cornerRadius : 0
                        topRightRadius:
                            segment.isLast ? Kirigami.Units.cornerRadius : 0
                        bottomRightRadius:
                            segment.isLast ? Kirigami.Units.cornerRadius : 0
                    }

                    color: segment.checked
                        ? Kirigami.Theme.alternateBackgroundColor
                        : Kirigami.Theme.backgroundColor

                    border {
                        width: 1
                        color: segment.highlighted
                            ? Kirigami.Theme.focusColor
                            : Kirigami.ColorUtils.linearInterpolation(
                                Kirigami.Theme.backgroundColor,
                                Kirigami.Theme.textColor,
                                Kirigami.Theme.frameContrast)
                    }

                    Behavior on color {
                        ColorAnimation {
                            duration: Kirigami.Units.shortDuration
                            easing.type: Easing.OutCubic
                        }
                    }
                    Behavior on border.color {
                        ColorAnimation {
                            duration: Kirigami.Units.shortDuration
                            easing.type: Easing.OutCubic
                        }
                    }
                }

                contentItem: RowLayout {
                    spacing: Kirigami.Units.smallSpacing

                    Item { Layout.fillWidth: true }

                    Kirigami.Icon {
                        source: segment.modelData.iconName
                        implicitWidth:
                            Kirigami.Units.iconSizes.sizeForLabels
                        implicitHeight:
                            Kirigami.Units.iconSizes.sizeForLabels
                        // Force monochrome rendering so the `color`
                        // binding wins for *both* segments. Without
                        // this, multi-colour mimetype icons like
                        // `video-x-generic` ignore the tint while
                        // monochrome ones (`view-media-recent`)
                        // recolour correctly — producing the
                        // "only one icon turns blue" asymmetry.
                        isMask: true
                        color: segment.checked
                            ? Kirigami.Theme.highlightColor
                            : Kirigami.Theme.textColor

                        Behavior on color {
                            ColorAnimation {
                                duration: Kirigami.Units.shortDuration
                                easing.type: Easing.OutCubic
                            }
                        }
                    }

                    QQC2.Label {
                        Layout.alignment: Qt.AlignVCenter
                        text: segment.modelData.text
                        color: Kirigami.Theme.textColor
                        // Keep weight constant across states — the
                        // background colour swap (bg ↔ alternateBg)
                        // and `focusColor` border already mark the
                        // checked segment clearly enough; bolding
                        // also shifts the label width and nudges
                        // adjacent content on every toggle.
                        elide: Text.ElideRight
                        Accessible.ignored: true
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }
    }
}
