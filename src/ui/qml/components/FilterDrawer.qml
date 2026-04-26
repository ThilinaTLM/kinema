// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Right-edge filter drawer for the Browse page. Hosts a
// `FormCard.FormCard` with one delegate per Browse VM filter:
// kind, date window, sort, rating, hide-obscure, multi-select
// genres, plus a "Reset filters" button at the bottom.
//
// `FormCard` rebuilds itself on each binding change which is fine
// here — the drawer stays constructed for the page's lifetime and
// the form size is small.
//
// The drawer is opened from the Browse page header's filter
// action; on narrow widths Kirigami collapses it to a modal
// overlay automatically.
Kirigami.OverlayDrawer {
    id: drawer
    edge: Qt.RightEdge
    modal: false
    width: Math.min(Kirigami.Units.gridUnit * 20,
        applicationWindow().width * 0.4)

    contentItem: ColumnLayout {
        spacing: 0

        Kirigami.Heading {
            text: i18nc("@title browse filter drawer", "Filters")
            level: 2
            Layout.margins: Kirigami.Units.largeSpacing
        }

        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth

            ColumnLayout {
                width: parent.width
                spacing: Kirigami.Units.largeSpacing

                FormCard.FormCard {
                    Layout.fillWidth: true

                    FormCard.FormComboBoxDelegate {
                        id: kindCombo
                        text: i18nc("@option:combobox", "Kind")
                        model: [
                            i18nc("@item kind", "Movies"),
                            i18nc("@item kind", "TV Series"),
                        ]
                        currentIndex: browseVm.kind
                        onCurrentIndexChanged: {
                            if (currentIndex !== browseVm.kind) {
                                browseVm.kind = currentIndex;
                            }
                        }
                    }
                    FormCard.FormDelegateSeparator { }

                    FormCard.FormComboBoxDelegate {
                        id: windowCombo
                        text: i18nc("@option:combobox", "Released")
                        // Indices match `core::DateWindow` enum
                        // declaration order (PastMonth=0,
                        // Past3Months=1, ThisYear=2, Past3Years=3,
                        // Any=4).
                        model: [
                            i18nc("@item date window", "Past month"),
                            i18nc("@item date window", "Past 3 months"),
                            i18nc("@item date window", "This year"),
                            i18nc("@item date window", "Past 3 years"),
                            i18nc("@item date window", "Any time"),
                        ]
                        currentIndex: browseVm.dateWindow
                        onCurrentIndexChanged: {
                            if (currentIndex !== browseVm.dateWindow) {
                                browseVm.dateWindow = currentIndex;
                            }
                        }
                    }
                    FormCard.FormDelegateSeparator { }

                    FormCard.FormComboBoxDelegate {
                        id: sortCombo
                        text: i18nc("@option:combobox", "Sort by")
                        // Indices match `api::DiscoverSort` (Popularity=0,
                        // ReleaseDate=1, Rating=2, TitleAsc=3).
                        model: [
                            i18nc("@item sort", "Most popular"),
                            i18nc("@item sort", "Newest first"),
                            i18nc("@item sort", "Highest rated"),
                            i18nc("@item sort", "Title (A–Z)"),
                        ]
                        currentIndex: browseVm.sort
                        onCurrentIndexChanged: {
                            if (currentIndex !== browseVm.sort) {
                                browseVm.sort = currentIndex;
                            }
                        }
                    }
                    FormCard.FormDelegateSeparator { }

                    FormCard.FormComboBoxDelegate {
                        id: ratingCombo
                        text: i18nc("@option:combobox", "Min rating")
                        model: [
                            { label: i18nc("@item rating", "Any"),    value: 0 },
                            { label: i18nc("@item rating", "★ 6.0+"), value: 60 },
                            { label: i18nc("@item rating", "★ 7.0+"), value: 70 },
                            { label: i18nc("@item rating", "★ 7.5+"), value: 75 },
                            { label: i18nc("@item rating", "★ 8.0+"), value: 80 },
                        ]
                        textRole: "label"
                        valueRole: "value"
                        currentIndex: {
                            switch (browseVm.minRatingPct) {
                            case 0:  return 0;
                            case 60: return 1;
                            case 70: return 2;
                            case 75: return 3;
                            case 80: return 4;
                            }
                            return 0;
                        }
                        onActivated: function (index) {
                            const v = model[index].value;
                            if (v !== browseVm.minRatingPct) {
                                browseVm.minRatingPct = v;
                            }
                        }
                    }
                    FormCard.FormDelegateSeparator { }

                    FormCard.FormSwitchDelegate {
                        text: i18nc("@option:check",
                            "Hide obscure titles")
                        description: i18nc("@info",
                            "Skip results with fewer than 200 ratings.")
                        checked: browseVm.hideObscure
                        onToggled: {
                            if (checked !== browseVm.hideObscure) {
                                browseVm.hideObscure = checked;
                            }
                        }
                    }
                }

                FormCard.FormHeader {
                    Layout.fillWidth: true
                    title: i18nc("@title:group browse filter section",
                        "Genres")
                }

                FormCard.FormCard {
                    Layout.fillWidth: true

                    Repeater {
                        model: browseVm.availableGenres

                        delegate: FormCard.FormCheckDelegate {
                            required property var modelData
                            text: modelData.name !== undefined
                                ? modelData.name : ""
                            checked: modelData.checked === true
                            onToggled: {
                                const id = modelData.id;
                                let next = browseVm.genreIds.slice();
                                const idx = next.indexOf(id);
                                if (checked && idx < 0) {
                                    next.push(id);
                                } else if (!checked && idx >= 0) {
                                    next.splice(idx, 1);
                                } else {
                                    return;
                                }
                                browseVm.genreIds = next;
                            }
                        }
                    }

                    FormCard.FormPlaceholderMessageDelegate {
                        visible: browseVm.availableGenres.length === 0
                        text: i18nc("@info genres loading",
                            "Loading genres…")
                        icon.name: "view-categories"
                    }
                }
            }
        }

        QQC2.Button {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            text: i18nc("@action:button", "Reset filters")
            icon.name: "edit-clear-history"
            onClicked: browseVm.resetFilters()
        }
    }
}
