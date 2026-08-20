// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KConfigGroup>
#include <KSharedConfig>

#include <QString>

namespace kinema::config::detail {

inline KConfigGroup group(const KSharedConfigPtr& cfg, const char* name)
{
    return cfg->group(QString::fromLatin1(name));
}

template <typename T>
inline T read(const KSharedConfigPtr& cfg, const char* groupName,
    const char* key, const T& def)
{
    return group(cfg, groupName).readEntry(key, def);
}

template <typename T>
inline void write(const KSharedConfigPtr& cfg, const char* groupName,
    const char* key, const T& value)
{
    auto g = group(cfg, groupName);
    g.writeEntry(key, value);
    g.sync();
}

inline void erase(const KSharedConfigPtr& cfg, const char* groupName,
    const char* key)
{
    auto g = group(cfg, groupName);
    g.deleteEntry(key);
    g.sync();
}

/// Read a string entry, falling back to `def` when it is absent or
/// empty (an empty stored value is treated as "unset").
inline QString readDefaulted(const KSharedConfigPtr& cfg,
    const char* groupName, const char* key, const QString& def)
{
    const auto raw = read(cfg, groupName, key, def);
    return raw.isEmpty() ? def : raw;
}

/// Trim `value`, falling back to `def` when the result is empty.
/// Used by setters that persist a user-editable URL.
inline QString normalizeUrl(QString value, const QString& def)
{
    const auto trimmed = value.trimmed();
    return trimmed.isEmpty() ? def : trimmed;
}

} // namespace kinema::config::detail
