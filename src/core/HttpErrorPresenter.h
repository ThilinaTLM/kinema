// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

#include <exception>

namespace kinema::core {

/**
 * Translate an exception thrown by HttpClient or a higher-level API
 * client into a user-facing message, and log it at warning level with
 * the supplied context tag.
 *
 * Collapses the duplicated try/catch pattern that callers used to
 * write out by hand:
 *
 * ```
 * try { ... } catch (const core::HttpError& e) {
 *     qCWarning(KINEMA) << "ctx:" << e.message();
 *     pane->showError(e.message());
 * } catch (const std::exception& e) {
 *     qCWarning(KINEMA) << "ctx (unknown):" << e.what();
 *     pane->showError(QString::fromUtf8(e.what()));
 * }
 * ```
 *
 * Now:
 *
 * ```
 * try { ... } catch (const std::exception& e) {
 *     pane->showError(core::describeError(e, "ctx"));
 * }
 * ```
 */
QString describeError(const std::exception& e, const char* context);

} // namespace kinema::core
