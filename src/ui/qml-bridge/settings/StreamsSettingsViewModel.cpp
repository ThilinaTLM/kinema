// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/StreamsSettingsViewModel.h"
#include "config/FilterSettings.h"
#include <KLocalizedString>

namespace kinema::ui::qml::settings {

// ============================== Streams (indexer-agnostic filters) =======

StreamsSettingsViewModel::StreamsSettingsViewModel(
    config::FilterSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

QStringList StreamsSettingsViewModel::excludedResolutions() const
{
    return m_settings.excludedResolutions();
}
QStringList StreamsSettingsViewModel::excludedCategories() const
{
    return m_settings.excludedCategories();
}
QString StreamsSettingsViewModel::blocklistText() const
{
    return m_settings.keywordBlocklist().join(QLatin1Char('\n'));
}

QVariantList StreamsSettingsViewModel::resolutionOptions() const
{
    QVariantList out;
    const QList<std::pair<QString, QString>> opts {
        { QStringLiteral("4k"), i18nc("@option resolution",
                                    "4K (2160p & 1440p)") },
        { QStringLiteral("1080p"), i18nc("@option resolution", "1080p") },
        { QStringLiteral("720p"), i18nc("@option resolution", "720p") },
        { QStringLiteral("480p"), i18nc("@option resolution", "480p") },
        { QStringLiteral("other"), i18nc("@option resolution",
                                       "Other / unknown") },
    };
    for (const auto& [token, label] : opts) {
        QVariantMap row;
        row.insert(QStringLiteral("token"), token);
        row.insert(QStringLiteral("label"), label);
        out.append(row);
    }
    return out;
}

QVariantList StreamsSettingsViewModel::categoryOptions() const
{
    QVariantList out;
    const QList<std::pair<QString, QString>> opts {
        { QStringLiteral("cam"), i18nc("@option variant", "CAM") },
        { QStringLiteral("scr"), i18nc("@option variant", "Screener") },
        { QStringLiteral("threed"), i18nc("@option variant", "3D") },
        { QStringLiteral("hdr"), i18nc("@option variant", "HDR") },
        { QStringLiteral("hdr10plus"), i18nc("@option variant", "HDR10+") },
        { QStringLiteral("dolbyvision"), i18nc("@option variant",
                                              "Dolby Vision") },
        { QStringLiteral("nonen"), i18nc("@option variant",
                                       "Non-English") },
        { QStringLiteral("unknown"), i18nc("@option variant",
                                         "Unknown quality") },
        { QStringLiteral("brremux"), i18nc("@option variant",
                                         "BluRay REMUX") },
    };
    for (const auto& [token, label] : opts) {
        QVariantMap row;
        row.insert(QStringLiteral("token"), token);
        row.insert(QStringLiteral("label"), label);
        out.append(row);
    }
    return out;
}

void StreamsSettingsViewModel::setExcludedResolutions(
    const QStringList& tokens)
{
    if (m_settings.excludedResolutions() == tokens) {
        return;
    }
    m_settings.setExcludedResolutions(tokens);
    Q_EMIT excludedResolutionsChanged();
}

void StreamsSettingsViewModel::setExcludedCategories(
    const QStringList& tokens)
{
    if (m_settings.excludedCategories() == tokens) {
        return;
    }
    m_settings.setExcludedCategories(tokens);
    Q_EMIT excludedCategoriesChanged();
}

void StreamsSettingsViewModel::setBlocklistText(const QString& text)
{
    QStringList keywords
        = text.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    for (auto& k : keywords) {
        k = k.trimmed();
    }
    if (m_settings.keywordBlocklist() == keywords) {
        return;
    }
    m_settings.setKeywordBlocklist(keywords);
    Q_EMIT blocklistChanged();
}

void StreamsSettingsViewModel::toggleResolution(const QString& token,
    bool on)
{
    auto next = excludedResolutions();
    const bool present = next.contains(token);
    if (on && !present) {
        next.append(token);
    } else if (!on && present) {
        next.removeAll(token);
    } else {
        return;
    }
    setExcludedResolutions(next);
}

void StreamsSettingsViewModel::toggleCategory(const QString& token,
    bool on)
{
    auto next = excludedCategories();
    const bool present = next.contains(token);
    if (on && !present) {
        next.append(token);
    } else if (!on && present) {
        next.removeAll(token);
    } else {
        return;
    }
    setExcludedCategories(next);
}

bool StreamsSettingsViewModel::resolutionExcluded(
    const QString& token) const
{
    return excludedResolutions().contains(token);
}

bool StreamsSettingsViewModel::categoryExcluded(
    const QString& token) const
{
    return excludedCategories().contains(token);
}

} // namespace kinema::ui::qml::settings
