// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QMetaType>
#include <QString>

namespace kinema::domain {

/// Which debrid provider the user has chosen as the active one.
/// `None` means the unified downloader uses the libtorrent backend
/// directly. Persisted in the `[Debrid]` KConfig group as a
/// stable string token; do not renumber.
enum class DebridProvider {
    None = 0,
    RealDebrid = 1,
    AllDebrid = 2,
};

/// Stable string token for KConfig persistence. Returns
/// `"none"` / `"realdebrid"` / `"alldebrid"`.
QString providerToString(DebridProvider p);

/// Inverse of `providerToString`. Unknown / empty values map to
/// `DebridProvider::None`.
DebridProvider providerFromString(const QString& s);

} // namespace kinema::domain

Q_DECLARE_METATYPE(kinema::domain::DebridProvider)
