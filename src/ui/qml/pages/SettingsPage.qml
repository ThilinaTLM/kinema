// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Settings host. Keep this as a real Kirigami.Page because it is pushed into
// the application shell's PageRow as a top-level navigation surface. The
// Kirigami Addons CategorizedSettings helper is itself a PageRow, and nesting
// a PageRow as an outer PageRow page trips Kirigami's global toolbar/ColumnView
// bookkeeping during root-page replacement.
Kirigami.Page {
    id: page
    objectName: "settings"

    readonly property var categories: [
        {
            key: "general",
            title: i18nc("@title:tab settings page", "General"),
            iconName: "preferences-other",
            url: Qt.resolvedUrl("settings/GeneralSettingsPage.qml")
        },
        {
            key: "tmdb",
            title: i18nc("@title:tab settings page", "TMDB (Discover)"),
            iconName: "applications-multimedia",
            url: Qt.resolvedUrl("settings/TmdbSettingsPage.qml")
        },
        {
            key: "realdebrid",
            title: i18nc("@title:tab settings page", "Real-Debrid"),
            iconName: "network-server",
            url: Qt.resolvedUrl("settings/RealDebridSettingsPage.qml")
        },
        {
            key: "filters",
            title: i18nc("@title:tab settings page", "Filters"),
            iconName: "view-filter",
            url: Qt.resolvedUrl("settings/FiltersSettingsPage.qml")
        },
        {
            key: "player",
            title: i18nc("@title:tab settings page", "Player"),
            iconName: "media-playback-start",
            url: Qt.resolvedUrl("settings/PlayerSettingsPage.qml")
        },
        {
            key: "subtitles",
            title: i18nc("@title:tab settings page", "Subtitles"),
            iconName: "media-view-subtitles-symbolic",
            url: Qt.resolvedUrl("settings/SubtitlesSettingsPage.qml")
        },
        {
            key: "appearance",
            title: i18nc("@title:tab settings page", "Appearance"),
            iconName: "preferences-desktop-theme",
            url: Qt.resolvedUrl("settings/AppearanceSettingsPage.qml")
        }
    ]

    property int selectedIndex: 0
    readonly property var selectedCategory: categories[selectedIndex]

    title: selectedCategory
        ? i18nc("@title:window", "Settings — %1", selectedCategory.title)
        : i18nc("@title:window", "Settings")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    function indexOfCategory(key) {
        for (let i = 0; i < categories.length; ++i) {
            if (categories[i].key === key) {
                return i;
            }
        }
        return -1;
    }

    function selectCategory(key) {
        const index = indexOfCategory(key);
        selectedIndex = index >= 0 ? index : 0;
    }

    Component.onCompleted: {
        if (settingsVm.initialCategory.length > 0) {
            selectCategory(settingsVm.initialCategory);
            Qt.callLater(() => settingsVm.clearInitialCategory());
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        QQC2.ScrollView {
            id: categoryPane
            Layout.fillHeight: true
            Layout.preferredWidth: Kirigami.Units.gridUnit * 13
            Layout.minimumWidth: Kirigami.Units.gridUnit * 10
            clip: true

            ListView {
                id: categoryList
                model: page.categories
                currentIndex: page.selectedIndex
                topMargin: Theme.inlineSpacing
                bottomMargin: Theme.inlineSpacing
                boundsBehavior: Flickable.StopAtBounds

                delegate: QQC2.ItemDelegate {
                    id: delegate

                    required property int index
                    required property var modelData

                    width: ListView.view.width
                    text: modelData.title
                    icon.name: modelData.iconName
                    checkable: true
                    checked: index === page.selectedIndex
                    highlighted: checked
                    onClicked: page.selectCategory(modelData.key)
                }
            }
        }

        Kirigami.Separator {
            Layout.fillHeight: true
        }

        Loader {
            id: settingsLoader
            Layout.fillWidth: true
            Layout.fillHeight: true
            source: page.selectedCategory ? page.selectedCategory.url : ""
            asynchronous: false
        }
    }
}
