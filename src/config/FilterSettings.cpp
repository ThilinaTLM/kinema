// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/FilterSettings.h"

#include "config/ConfigAccess.h"

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
    return detail::read(m_config, kGroup, kKeyExcludedResolutions,
        QStringList {});
}

void FilterSettings::setExcludedResolutions(QStringList list)
{
    list = normalize(std::move(list));
    if (excludedResolutions() == list) {
        return;
    }
    detail::write(m_config, kGroup, kKeyExcludedResolutions, list);
    Q_EMIT exclusionsChanged();
}

QStringList FilterSettings::excludedCategories() const
{
    return detail::read(m_config, kGroup, kKeyExcludedCategories,
        QStringList {});
}

void FilterSettings::setExcludedCategories(QStringList list)
{
    list = normalize(std::move(list));
    if (excludedCategories() == list) {
        return;
    }
    detail::write(m_config, kGroup, kKeyExcludedCategories, list);
    Q_EMIT exclusionsChanged();
}

QStringList FilterSettings::keywordBlocklist() const
{
    return detail::read(m_config, kGroup, kKeyKeywordBlocklist,
        QStringList {});
}

void FilterSettings::setKeywordBlocklist(QStringList list)
{
    list = normalize(std::move(list));
    if (keywordBlocklist() == list) {
        return;
    }
    detail::write(m_config, kGroup, kKeyKeywordBlocklist, list);
    Q_EMIT keywordBlocklistChanged(list);
}

} // namespace kinema::config
