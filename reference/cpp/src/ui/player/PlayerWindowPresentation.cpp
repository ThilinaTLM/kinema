// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/PlayerWindowPresentation.h"

#ifdef KINEMA_HAVE_LIBMPV

namespace kinema::ui::player::window_presentation {

PlayRequest forPlayback(bool alreadyVisible) noexcept
{
    if (alreadyVisible) {
        return {};
    }
    return PlayRequest {
        .showWindow = true,
        .raiseWindow = true,
        .requestActivation = true,
    };
}

} // namespace kinema::ui::player::window_presentation

#endif // KINEMA_HAVE_LIBMPV
