// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

namespace kinema::ui::qml::settings {

/// Compare against `current` and only apply `apply` when `value`
/// differs. Returns true when a change was applied so the caller can
/// emit its own `NOTIFY` signal (and any extra side-effects).
///
/// Consolidates the repetitive "if (get() == v) return; set(v);
/// emit();" write-through setters used by the settings view-models.
template <typename Settings, typename T>
bool setIfChanged(Settings& settings, const T& value,
    T (Settings::*current)() const, void (Settings::*apply)(T))
{
    if ((settings.*current)() == value) {
        return false;
    }
    (settings.*apply)(value);
    return true;
}

/// Overload for setters that take a `const T&` (e.g. `QString`).
template <typename Settings, typename T>
bool setIfChanged(Settings& settings, const T& value,
    T (Settings::*current)() const, void (Settings::*apply)(const T&))
{
    if ((settings.*current)() == value) {
        return false;
    }
    (settings.*apply)(value);
    return true;
}

} // namespace kinema::ui::qml::settings
