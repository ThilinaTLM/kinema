// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Disk-cache budgets. Currently only `subtitleBudgetMb` is in use; a
 * future video / poster cache will live alongside under the same
 * `[Cache]` KConfig group.
 *
 * Defaults:
 *   subtitleBudgetMb     200    (huge headroom — subs are tiny)
 */
class CacheSettings : public QObject
{
    Q_OBJECT
public:
    explicit CacheSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    int subtitleBudgetMb() const;
    void setSubtitleBudgetMb(int);

Q_SIGNALS:
    void subtitleBudgetMbChanged(int);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
