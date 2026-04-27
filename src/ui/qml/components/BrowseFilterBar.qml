// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Single-row sticky filter bar for Browse. Every filter the user
// can tweak lives directly on the bar — there is no overflow
// sheet. Layout (left to right):
//
//   * Movies / TV Series  — `MediaKindSwitch` (segmented pill).
//   * Genres ▾            — `GenresMenuButton` popup.
//   * Released ▾          — `QQC2.ComboBox` over the 5 date windows.
//   * ★ Min ▾             — `QQC2.ToolButton` whose attached popup
//                           hosts a 0..90 step-5 rating slider.
//   * <fill>              — pushes the right-side group.
//   * Sort ▾              — native `QQC2.ComboBox`.
//   * Hide obscure        — checkable `QQC2.ToolButton`.
//
// All controls are theme-driven — no hard-coded sizes/colours —
// and `QQC2.ToolBar` collapses any overflow on narrow windows
// into its built-in ▸ menu.
QQC2.ToolBar {
    id: bar

    Kirigami.Theme.colorSet: Kirigami.Theme.Header
    Kirigami.Theme.inherit: false

    // Keep filter chrome aligned with the app-wide page gutter while
    // retaining compact toolbar height.
    padding: Theme.inlineSpacing
    leftPadding: Theme.pageMargin
    rightPadding: Theme.pageMargin

    contentItem: RowLayout {
        spacing: Theme.groupSpacing

        MediaKindSwitch {
            kind: browseVm.kind
            onActivated: function (newKind) {
                if (browseVm.kind !== newKind) {
                    browseVm.kind = newKind;
                }
            }
        }

        GenresMenuButton {
            sourceGenres: browseVm.availableGenres
            selectedIds: browseVm.genreIds
            onSelectionChanged: function (nextIds) {
                browseVm.genreIds = nextIds;
            }
        }

        // ---- Released ---------------------------------------------
        QQC2.ComboBox {
            id: releasedCombo
            // Indices match the radio order in the (now-removed)
            // `BrowseFiltersSheet`: Past month / Past 3 months /
            // This year / Past 3 years / Any time.
            model: [
                i18nc("@item date window", "Past month"),
                i18nc("@item date window", "Past 3 months"),
                i18nc("@item date window", "This year"),
                i18nc("@item date window", "Past 3 years"),
                i18nc("@item date window", "Any time")
            ]
            Accessible.name: i18nc("@label", "Released")
            currentIndex: browseVm.dateWindow
            onActivated: if (currentIndex !== browseVm.dateWindow) {
                browseVm.dateWindow = currentIndex;
            }
        }

        // ---- Min rating popup ------------------------------------
        QQC2.ToolButton {
            id: ratingBtn
            text: browseVm.minRatingPct <= 0
                ? i18nc("@label rating filter", "★ Any+")
                : i18nc("@label rating filter", "★ %1+",
                    (browseVm.minRatingPct / 10).toFixed(1))
            display: QQC2.AbstractButton.TextOnly
            checkable: false
            Accessible.name: i18nc("@label", "Minimum rating")
            onClicked: ratingPopup.visible
                ? ratingPopup.close()
                : ratingPopup.open()

            QQC2.Popup {
                id: ratingPopup
                y: ratingBtn.height
                padding: Theme.groupSpacing
                modal: false
                focus: true

                contentItem: ColumnLayout {
                    spacing: Theme.inlineSpacing
                    implicitWidth: Kirigami.Units.gridUnit * 16

                    RowLayout {
                        Layout.fillWidth: true
                        QQC2.Label {
                            Layout.fillWidth: true
                            text: i18nc("@title:group browse filter section",
                                "Minimum rating")
                            font.bold: true
                        }
                        QQC2.Label {
                            text: ratingSlider.value <= 0
                                ? i18nc("@label rating slider readout",
                                    "Any")
                                : i18nc("@label rating slider readout",
                                    "★ %1+",
                                    (ratingSlider.value / 10).toFixed(1))
                            color: Kirigami.Theme.textColor
                            font.weight: Font.DemiBold
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
            }
        }

        Item { Layout.fillWidth: true }

        // ---- Sort -------------------------------------------------
        QQC2.ComboBox {
            id: sortCombo
            // Indices match `api::DiscoverSort` (Popularity=0,
            // ReleaseDate=1, Rating=2, TitleAsc=3) so we bind the
            // index directly to `browseVm.sort`.
            model: [
                i18nc("@item sort", "Most popular"),
                i18nc("@item sort", "Newest first"),
                i18nc("@item sort", "Highest rated"),
                i18nc("@item sort", "Title (A\u2013Z)")
            ]
            Accessible.name: i18nc("@label", "Sort by")
            currentIndex: browseVm.sort
            onActivated: if (currentIndex !== browseVm.sort) {
                browseVm.sort = currentIndex;
            }
        }

        // ---- Hide obscure ----------------------------------------
        QQC2.ToolButton {
            text: i18nc("@option:check", "Hide obscure")
            icon.name: "view-filter"
            display: QQC2.AbstractButton.TextBesideIcon
            checkable: true
            checked: browseVm.hideObscure
            QQC2.ToolTip.text: i18nc("@info:tooltip",
                "Skip results with fewer than 200 ratings.")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onToggled: if (checked !== browseVm.hideObscure) {
                browseVm.hideObscure = checked;
            }
        }
    }
}
