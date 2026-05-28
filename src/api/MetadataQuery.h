// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>
#include <QStringView>

#include <optional>

namespace kinema::api::metadata_query {

/// Extract a normalized IMDb title id (`tt...`) from either a bare id
/// or an IMDb title URL. Returns std::nullopt for ordinary title text.
std::optional<QString> extractImdbTitleId(QStringView input);

/// True when `input` is a bare IMDb title id or supported IMDb title URL.
bool isImdbTitleId(QStringView input);

} // namespace kinema::api::metadata_query
