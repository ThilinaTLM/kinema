// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/MpvLoadCommand.h"

#ifdef KINEMA_HAVE_LIBMPV

namespace kinema::ui::player::mpv_command {

QStringList buildLoadFileCommand(const QUrl& url,
    std::optional<double> startSeconds)
{
    QStringList args;
    args << QStringLiteral("loadfile")
         << QString::fromUtf8(url.toEncoded());
    if (startSeconds && *startSeconds > 0.0) {
        // loadfile <url> [<flags> [<index> [<options>]]]; index must be an
        // integer (-1 = unused for non-insert flags), options is a
        // comma-separated key=value list applied to the loaded file.
        args << QStringLiteral("replace")
             << QStringLiteral("-1")
             << QStringLiteral("start=%1").arg(*startSeconds, 0, 'f', 3);
    }
    return args;
}

} // namespace kinema::ui::player::mpv_command

#endif // KINEMA_HAVE_LIBMPV
