// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/SearchSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "General";
constexpr auto kKey = "searchKind";
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


} // namespace kinema::config
