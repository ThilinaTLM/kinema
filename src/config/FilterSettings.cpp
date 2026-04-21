// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/FilterSettings.h"

#include <KConfigGroup>

namespace kinema::config {

namespace {

constexpr auto kGroup = "Filters";
constexpr auto kKeyExcludedResolutions = "excludedResolutions";
constexpr auto kKeyExcludedCategories = "excludedCategories";
constexpr auto kKeyKeywordBlocklist = "keywordBlocklist";

QStringList normalize(QStringList list)
{
    QStringList out;
    out.reserve(list.size());
    for (auto& s : list) {
        s = s.trimmed();
        if (!s.isEmpty()) {
            out.append(std::move(s));
        }
    }
    return out;
}

} // namespace

FilterSettings::FilterSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

QStringList FilterSettings::excludedResolutions() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyExcludedResolutions, QStringList {});
}

void FilterSettings::setExcludedResolutions(QStringList list)
{
    list = normalize(std::move(list));
    if (excludedResolutions() == list) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyExcludedResolutions, list);
    g.sync();
    Q_EMIT exclusionsChanged();
}

QStringList FilterSettings::excludedCategories() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyExcludedCategories, QStringList {});
}

void FilterSettings::setExcludedCategories(QStringList list)
{
    list = normalize(std::move(list));
    if (excludedCategories() == list) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyExcludedCategories, list);
    g.sync();
    Q_EMIT exclusionsChanged();
}

QStringList FilterSettings::keywordBlocklist() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyKeywordBlocklist, QStringList {});
}

void FilterSettings::setKeywordBlocklist(QStringList list)
{
    list = normalize(std::move(list));
    if (keywordBlocklist() == list) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyKeywordBlocklist, list);
    g.sync();
    Q_EMIT keywordBlocklistChanged(list);
}

} // namespace kinema::config
