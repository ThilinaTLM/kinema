// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QStringList>
#include <QVariantList>

namespace kinema::config {
class FilterSettings;
}

namespace kinema::ui::qml::settings {

class StreamsSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QStringList excludedResolutions READ excludedResolutions
        WRITE setExcludedResolutions NOTIFY excludedResolutionsChanged)
    Q_PROPERTY(QStringList excludedCategories READ excludedCategories
        WRITE setExcludedCategories NOTIFY excludedCategoriesChanged)
    Q_PROPERTY(QString blocklistText READ blocklistText
        WRITE setBlocklistText NOTIFY blocklistChanged)
    Q_PROPERTY(QVariantList resolutionOptions READ resolutionOptions CONSTANT)
    Q_PROPERTY(QVariantList categoryOptions READ categoryOptions CONSTANT)

public:
    StreamsSettingsViewModel(config::FilterSettings& settings,
        QObject* parent = nullptr);

    QStringList excludedResolutions() const;
    QStringList excludedCategories() const;
    QString blocklistText() const;
    QVariantList resolutionOptions() const;
    QVariantList categoryOptions() const;

    void setExcludedResolutions(const QStringList& tokens);
    void setExcludedCategories(const QStringList& tokens);
    void setBlocklistText(const QString& text);

    /// Toggle helpers for QML check delegates.
    Q_INVOKABLE void toggleResolution(const QString& token, bool on);
    Q_INVOKABLE void toggleCategory(const QString& token, bool on);
    Q_INVOKABLE bool resolutionExcluded(const QString& token) const;
    Q_INVOKABLE bool categoryExcluded(const QString& token) const;

Q_SIGNALS:
    void excludedResolutionsChanged();
    void excludedCategoriesChanged();
    void blocklistChanged();

private:
    config::FilterSettings& m_settings;
};

} // namespace kinema::ui::qml::settings
