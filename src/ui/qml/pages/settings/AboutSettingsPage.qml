// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

Kirigami.Page {
    id: page

    property var aboutData

    title: aboutData && aboutData.displayName
        ? i18nc("@title:tab settings page", "About %1", aboutData.displayName)
        : i18nc("@title:tab settings page", "About")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    function joinNonEmpty(parts) {
        return parts.filter(function (part) {
            return part !== undefined && part !== null && part.length > 0;
        }).join("\n");
    }

    function personDescription(person) {
        return joinNonEmpty([
            person.task || "",
            person.emailAddress || "",
            person.webAddress || ""
        ]);
    }

    function bugReportUrl() {
        if (!aboutData || !aboutData.bugAddress || aboutData.bugAddress.length === 0) {
            return "";
        }
        if (aboutData.bugAddress.indexOf("@") >= 0) {
            return `mailto:${aboutData.bugAddress}`;
        }
        return aboutData.bugAddress;
    }

    QQC2.ScrollView {
        id: scrollView
        anchors.fill: parent
        clip: true

        contentWidth: availableWidth

        ColumnLayout {
            width: scrollView.availableWidth
            spacing: 0

            FormCard.FormCard {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.largeSpacing * 2

                FormCard.AbstractFormDelegate {
                    background: null
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing

                        Kirigami.Icon {
                            Layout.preferredWidth: Kirigami.Units.iconSizes.enormous
                            Layout.preferredHeight: width
                            Layout.maximumWidth: Math.min(page.width / 3,
                                Kirigami.Units.iconSizes.enormous)
                            source: Kirigami.Settings.applicationWindowIcon
                                || page.aboutData.programLogo
                                || page.aboutData.programIconName
                                || page.aboutData.componentName
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing

                            Kirigami.Heading {
                                Layout.fillWidth: true
                                text: page.aboutData.displayName.length > 0
                                    ? `${page.aboutData.displayName} ${page.aboutData.version}`
                                    : page.aboutData.version
                                wrapMode: Text.WordWrap
                            }

                            Kirigami.Heading {
                                Layout.fillWidth: true
                                level: 3
                                type: Kirigami.Heading.Type.Secondary
                                text: page.aboutData.shortDescription || ""
                                wrapMode: Text.WordWrap
                                visible: text.length > 0
                            }
                        }
                    }
                }

                FormCard.FormDelegateSeparator {}

                FormCard.FormTextDelegate {
                    visible: page.aboutData.copyrightStatement
                        && page.aboutData.copyrightStatement.length > 0
                    text: i18nc("@label", "Copyright")
                    descriptionItem.textFormat: Text.PlainText
                    description: page.aboutData.copyrightStatement
                }
            }

            FormCard.FormHeader {
                visible: page.aboutData.otherText
                    && page.aboutData.otherText.length > 0
                title: i18nc("@title:group", "Description")
            }

            FormCard.FormCard {
                visible: page.aboutData.otherText
                    && page.aboutData.otherText.length > 0

                FormCard.FormTextDelegate {
                    text: page.aboutData.otherText
                    description: ""
                    textItem.wrapMode: Text.WordWrap
                }
            }

            FormCard.FormHeader {
                visible: (page.aboutData.homepage
                        && page.aboutData.homepage.length > 0)
                    || bugButton.visible
                title: i18nc("@title:group", "Links")
            }

            FormCard.FormCard {
                visible: (page.aboutData.homepage
                        && page.aboutData.homepage.length > 0)
                    || bugButton.visible

                FormCard.FormButtonDelegate {
                    visible: page.aboutData.homepage
                        && page.aboutData.homepage.length > 0
                    text: i18nc("@action:button", "Homepage")
                    description: page.aboutData.homepage
                    icon.source: AppIcons.url("external-link")
                    icon.color: AppIcons.controlColor(enabled, false)
                    onClicked: Qt.openUrlExternally(page.aboutData.homepage)
                }

                FormCard.FormButtonDelegate {
                    id: bugButton
                    visible: page.bugReportUrl().length > 0
                    text: i18nc("@action:button", "Report issues")
                    description: page.aboutData.bugAddress || ""
                    icon.source: AppIcons.url("external-link")
                    icon.color: AppIcons.controlColor(enabled, false)
                    onClicked: Qt.openUrlExternally(page.bugReportUrl())
                }
            }

            FormCard.FormHeader {
                visible: page.aboutData.authors !== undefined
                    && page.aboutData.authors.length > 0
                title: i18nc("@title:group", "Authors")
            }

            FormCard.FormCard {
                visible: page.aboutData.authors !== undefined
                    && page.aboutData.authors.length > 0

                Repeater {
                    model: page.aboutData.authors

                    delegate: FormCard.FormTextDelegate {
                        text: modelData.name || ""
                        description: page.personDescription(modelData)
                    }
                }
            }

            FormCard.FormHeader {
                visible: page.aboutData.credits !== undefined
                    && page.aboutData.credits.length > 0
                title: i18nc("@title:group", "Credits")
            }

            FormCard.FormCard {
                visible: page.aboutData.credits !== undefined
                    && page.aboutData.credits.length > 0

                Repeater {
                    model: page.aboutData.credits

                    delegate: FormCard.FormTextDelegate {
                        text: modelData.name || ""
                        description: page.personDescription(modelData)
                    }
                }
            }

            FormCard.FormHeader {
                visible: page.aboutData.translators !== undefined
                    && page.aboutData.translators.length > 0
                title: i18nc("@title:group", "Translators")
            }

            FormCard.FormCard {
                visible: page.aboutData.translators !== undefined
                    && page.aboutData.translators.length > 0

                Repeater {
                    model: page.aboutData.translators

                    delegate: FormCard.FormTextDelegate {
                        text: modelData.name || ""
                        description: page.personDescription(modelData)
                    }
                }
            }

            FormCard.FormHeader {
                visible: page.aboutData.licenses !== undefined
                    && page.aboutData.licenses.length > 0
                title: page.aboutData.licenses.length === 1
                    ? i18nc("@title:group", "License")
                    : i18nc("@title:group", "Licenses")
            }

            FormCard.FormCard {
                visible: page.aboutData.licenses !== undefined
                    && page.aboutData.licenses.length > 0

                Repeater {
                    model: page.aboutData.licenses

                    delegate: FormCard.FormTextDelegate {
                        text: modelData.spdx && modelData.spdx.length > 0
                            ? `${modelData.name} (${modelData.spdx})`
                            : (modelData.name || "")
                        descriptionItem.textFormat: Text.PlainText
                        description: modelData.text || ""
                    }
                }
            }

            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.largeSpacing * 2
            }
        }
    }
}
