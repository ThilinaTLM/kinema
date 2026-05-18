// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QUrl>

#include <functional>

namespace kinema::core::io {

/**
 * Result of a `core::io::openExternal` job. The callback fires
 * once on the calling thread; `ok` is true on a successful handoff
 * to `KIO::OpenUrlJob`, false otherwise.
 *
 * `errorString` carries the KIO error text on failure; empty on
 * success.
 */
struct OpenExternalResult {
    bool ok = false;
    QString errorString;
};

using OpenExternalCallback = std::function<void(const OpenExternalResult&)>;

/**
 * Hand `url` off to the desktop's default handler via
 * `KIO::OpenUrlJob` (executable URLs are refused). The job parents
 * itself to `parent` for ownership; `callback` (if set) fires on
 * completion with success / error info so the caller can surface a
 * status message.
 *
 * Single entry point for every "open in external app" code path —
 * `StreamActions` (open magnet / open direct URL) and
 * `ShellViewModel` (open IMDb / TMDb / arbitrary URL) both route
 * through here so failure handling stays consistent.
 */
void openExternal(const QUrl& url,
    QObject* parent,
    OpenExternalCallback callback = {});

} // namespace kinema::core::io
