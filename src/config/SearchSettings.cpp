// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/SearchSettings.h"

#include <KConfigGroup>

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
    const auto s = m_config->group(QString::fromLatin1(kGroup))
                       .readEntry(kKey, QStringLiteral("Movie"));
    return s == QLatin1String("Series")
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
}

void SearchSettings::setKind(api::MediaKind k)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKey,
        k == api::MediaKind::Series
            ? QStringLiteral("Series")
            : QStringLiteral("Movie"));
    g.sync();
}

QStringList SearchSettings::recentQueries() const
{
    return m_config->group(QString::fromLatin1(kRecentGroup))
        .readEntry(kRecentKey, QStringList{});
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

    auto g = m_config->group(QString::fromLatin1(kRecentGroup));
    g.writeEntry(kRecentKey, list);
    g.sync();
    Q_EMIT recentQueriesChanged();
}

void SearchSettings::clearRecentQueries()
{
    auto g = m_config->group(QString::fromLatin1(kRecentGroup));
    g.deleteEntry(kRecentKey);
    g.sync();
    Q_EMIT recentQueriesChanged();
}

} // namespace kinema::config
