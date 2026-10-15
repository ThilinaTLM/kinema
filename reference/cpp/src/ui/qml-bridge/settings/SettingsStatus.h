// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

namespace kinema::ui::qml::settings {

// `Kirigami.MessageType` int-equivalents kept local so QML can use
// the small integer literals without dragging Kirigami into C++.
// (Kirigami's enum has Warning = 2 between Positive and Error; we
//  don't surface warnings in any of these forms.)
inline constexpr int kStatusInfo = 0;
inline constexpr int kStatusPositive = 1;
inline constexpr int kStatusError = 3;

} // namespace kinema::ui::qml::settings
