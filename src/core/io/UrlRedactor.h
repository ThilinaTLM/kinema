// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>
#include <QUrl>

namespace kinema::core {

/// Return a URL string safe for diagnostic logs and user-visible error text.
///
/// The app talks to services that sometimes place credentials in URL paths
/// (not just headers or query strings), so QUrl::RemoveQuery is not enough.
/// This helper keeps host/path context while stripping userinfo, query strings,
/// fragments, and known secret-bearing path tokens.
QString redactUrlForLog(const QUrl& url);

} // namespace kinema::core
