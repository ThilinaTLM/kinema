// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

class QQmlApplicationEngine;

namespace kinema::app {
class ServiceContainer;
}

namespace kinema::ui::qml {

class ShellViewModel;

/**
 * Registers the `image://kinema/...` image provider, exposes every
 * long-lived QObject the QML engine needs as a context property
 * (page view-models + the shell itself), and installs
 * `KLocalizedContext` so QML can use `i18n(...)`.
 *
 * Must run **before** `engine.load(...)`; otherwise the first wave
 * of property bindings re-evaluate against a null context property
 * and the engine logs `Cannot read property 'X' of null`.
 */
void installQmlContext(QQmlApplicationEngine& engine,
    app::ServiceContainer& services, ShellViewModel& shell);

} // namespace kinema::ui::qml
