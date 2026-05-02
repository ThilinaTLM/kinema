// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QStringList>
#include <QUrl>

#include <optional>

namespace kinema::ui::player::mpv_command {

QStringList buildLoadFileCommand(const QUrl& url,
    std::optional<double> startSeconds = std::nullopt);

} // namespace kinema::ui::player::mpv_command

#endif // KINEMA_HAVE_LIBMPV
