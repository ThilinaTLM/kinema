// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/CacheSettings.h"

#include "config/ConfigAccess.h"

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
    return qMax(1, detail::read(m_config, kGroup, kSubtitleBudget,
                       kDefaultSubtitleBudgetMb));
}

void CacheSettings::setSubtitleBudgetMb(int mb)
{
    mb = qMax(1, mb);
    if (subtitleBudgetMb() == mb) {
        return;
    }
    detail::write(m_config, kGroup, kSubtitleBudget, mb);
    Q_EMIT subtitleBudgetMbChanged(mb);
}

} // namespace kinema::config
