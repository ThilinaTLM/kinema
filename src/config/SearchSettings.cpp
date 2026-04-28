// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/SearchSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "General";
constexpr auto kKey = "searchKind";
constexpr auto kRecentGroup = "Search";
constexpr auto kRecentKey = "recentQueries";
constexpr int kRecentCap = 8;
} // namespace

SearchSettings::SearchSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

api::MediaKind SearchSettings::kind() const
{
    const auto s = detail::read(m_config, kGroup, kKey,
        QStringLiteral("Movie"));
    return s == QLatin1String("Series")
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
}

void SearchSettings::setKind(api::MediaKind k)
{
    detail::write(m_config, kGroup, kKey,
        k == api::MediaKind::Series
            ? QStringLiteral("Series")
            : QStringLiteral("Movie"));
}

QStringList SearchSettings::recentQueries() const
{
    return detail::read(m_config, kRecentGroup, kRecentKey, QStringList {});
}

void SearchSettings::addRecentQuery(const QString& q)
{
    const auto trimmed = q.trimmed();
    if (trimmed.isEmpty()) {
        return;
    }

    auto list = recentQueries();
    list.removeAll(trimmed);
    list.prepend(trimmed);
    while (list.size() > kRecentCap) {
        list.removeLast();
    }

    detail::write(m_config, kRecentGroup, kRecentKey, list);
    Q_EMIT recentQueriesChanged();
}

void SearchSettings::clearRecentQueries()
{
    detail::erase(m_config, kRecentGroup, kRecentKey);
    Q_EMIT recentQueriesChanged();
}

} // namespace kinema::config
