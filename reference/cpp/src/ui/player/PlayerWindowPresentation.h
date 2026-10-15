// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

namespace kinema::ui::player::window_presentation {

struct PlayRequest {
    bool showWindow = false;
    bool raiseWindow = false;
    bool requestActivation = false;
};

[[nodiscard]] PlayRequest forPlayback(bool alreadyVisible) noexcept;

} // namespace kinema::ui::player::window_presentation

#endif // KINEMA_HAVE_LIBMPV
