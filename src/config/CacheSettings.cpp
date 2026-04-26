// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/CacheSettings.h"

#include <KConfigGroup>

namespace kinema::config {

namespace {
constexpr auto kGroup = "Cache";
constexpr auto kSubtitleBudget = "subtitleBudgetMb";
constexpr int kDefaultSubtitleBudgetMb = 200;
} // namespace

CacheSettings::CacheSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

int CacheSettings::subtitleBudgetMb() const
{
    const auto v = m_config->group(QString::fromLatin1(kGroup))
                       .readEntry(kSubtitleBudget,
                           kDefaultSubtitleBudgetMb);
    return qMax(1, v);
}

void CacheSettings::setSubtitleBudgetMb(int mb)
{
    mb = qMax(1, mb);
    if (subtitleBudgetMb() == mb) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kSubtitleBudget, mb);
    g.sync();
    Q_EMIT subtitleBudgetMbChanged(mb);
}

} // namespace kinema::config
