// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Focused secondary-filters dialog for Browse. Houses the filters
// that are tweaked less often than kind / sort / genre and would
// otherwise clutter the inline `BrowseFilterBar`:
//
//   * Date window — radio group (Past month / Past 3 months /
//     This year / Past 3 years / Any time).
//   * Min rating  — slider 0..90 in 5-pt steps with a live "★ N.N+"
//     label. Replaces the previous combo (which forced 5 fixed
//     buckets) with a continuous picker the BrowseViewModel already
//     accepts via `setMinRatingPct(int)`.
//   * Hide obscure titles — switch with description.
//
// Footer carries Reset / Done. Reset fires `browseVm.resetFilters()`
// which clears every filter (including kind, genres, sort) — this
// matches the long-standing semantics of the action and keeps a
// single big-red-button affordance available somewhere on the page.
//
// Parented to the application overlay per Kirigami guidance for
// `OverlaySheet`.
Kirigami.OverlaySheet {
    id: sheet

    parent: applicationWindow().overlay

    header: Kirigami.Heading {
        level: 2
        text: i18nc("@title browse filter sheet", "More filters")
    }

    ColumnLayout {
        id: contentColumn

        // Pin a sensible width so OverlaySheet can compute its size
        // hint without bouncing between parent and content. We set
        // both `implicitWidth` and `Layout.preferredWidth` because
        // OverlaySheet's `implicitWidth`/`implicitHeight` blocks
        // (templates/OverlaySheet.qml) check both. Pinning a stable
        // size hint that does not depend on the contentItem's
        // actual width breaks the chain that previously triggered
        // a "Binding loop detected for property 'implicitHeight'"
        // warning every time the sheet was relaid out.
        readonly property real targetWidth: Math.min(
            applicationWindow().width - Kirigami.Units.gridUnit * 4,
            Kirigami.Units.gridUnit * 26)
        implicitWidth: targetWidth
        Layout.preferredWidth: targetWidth
        spacing: Kirigami.Units.largeSpacing

        // ---- Date window -----------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Heading {
                level: 4
                text: i18nc("@title:group browse filter section",
                    "Released")
            }

            QQC2.ButtonGroup { id: windowGroup }

            Repeater {
                model: [
                    { value: 0,
                      text: i18nc("@item date window", "Past month") },
                    { value: 1,
                      text: i18nc("@item date window", "Past 3 months") },
                    { value: 2,
                      text: i18nc("@item date window", "This year") },
                    { value: 3,
                      text: i18nc("@item date window", "Past 3 years") },
                    { value: 4,
                      text: i18nc("@item date window", "Any time") }
                ]

                delegate: QQC2.RadioButton {
                    required property var modelData
                    Layout.fillWidth: true
                    text: modelData.text
                    QQC2.ButtonGroup.group: windowGroup
                    checked: browseVm.dateWindow === modelData.value
                    onToggled: if (checked
                        && browseVm.dateWindow !== modelData.value) {
                        browseVm.dateWindow = modelData.value;
                    }
                }
            }
        }

        Kirigami.Separator { Layout.fillWidth: true }

        // ---- Minimum rating --------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.fillWidth: true
                Kirigami.Heading {
                    level: 4
                    text: i18nc("@title:group browse filter section",
                        "Minimum rating")
                    Layout.fillWidth: true
                }
                QQC2.Label {
                    text: ratingSlider.value <= 0
                        ? i18nc("@label rating slider readout",
                            "Any")
                        : i18nc("@label rating slider readout",
                            "\u2605 %1+",
                            (ratingSlider.value / 10).toFixed(1))
                    font.weight: Font.DemiBold
                    color: Kirigami.Theme.textColor
                }
            }

            QQC2.Slider {
                id: ratingSlider
                Layout.fillWidth: true
                from: 0
                to: 90
                stepSize: 5
                snapMode: QQC2.Slider.SnapAlways
                value: browseVm.minRatingPct
                onMoved: if (value !== browseVm.minRatingPct) {
                    browseVm.minRatingPct = value;
                }
            }
        }

        Kirigami.Separator { Layout.fillWidth: true }

        // ---- Hide obscure ----------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                QQC2.Label {
                    text: i18nc("@option:check",
                        "Hide obscure titles")
                    font.weight: Font.DemiBold
                }
                QQC2.Label {
                    text: i18nc("@info",
                        "Skip results with fewer than 200 ratings.")
                    font: Kirigami.Theme.smallFont
                    color: Kirigami.Theme.disabledTextColor
                    // Use elide instead of wrap: wrapping makes
                    // implicitHeight depend on actual width, which
                    // feeds back through OverlaySheet's size-hint
                    // calculation and triggers a binding-loop
                    // warning. The description is short and fits
                    // comfortably at our pinned sheet width.
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            QQC2.Switch {
                checked: browseVm.hideObscure
                onToggled: if (checked !== browseVm.hideObscure) {
                    browseVm.hideObscure = checked;
                }
            }
        }
    }

    footer: RowLayout {
        spacing: Kirigami.Units.smallSpacing

        QQC2.Button {
            text: i18nc("@action:button", "Reset filters")
            icon.name: "edit-clear-history"
            onClicked: {
                browseVm.resetFilters();
                sheet.close();
            }
        }
        Item { Layout.fillWidth: true }
        QQC2.Button {
            text: i18nc("@action:button", "Done")
            icon.name: "dialog-ok"
            onClicked: sheet.close()
        }
    }
}
