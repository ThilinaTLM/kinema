// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * SearchBar defaults.
 *
 * KConfig group: [General]
 * Keys:
 *   searchKind        "Movie" | "Series"
 */
class SearchSettings : public QObject
{
    Q_OBJECT
public:
    explicit SearchSettings(KSharedConfigPtr config, QObject* parent = nullptr);

    domain::MediaKind kind() const;
    void setKind(domain::MediaKind);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
