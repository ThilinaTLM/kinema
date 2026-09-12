// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "services/StreamActions.h"
#include "ui/qml-bridge/streams/StreamsListModel.h"

namespace kinema::ui::qml {

/// Resolve the stream at `row` and route it to one of
/// `StreamActions`'s utility commands (`copyMagnet`,
/// `openMagnet`, `copyDirectUrl`, `openDirectUrl`,
/// `copyReleaseName`). Shared by the movie and series detail
/// view-models, which used to each carry their own copy.
template <typename Method>
void dispatchStreamAction(StreamsListModel* streams,
                          services::StreamActions* actions,
                          int row,
                          Method method)
{
    if (!actions) {
        return;
    }
    if (const auto* s = streams->at(row)) {
        (actions->*method)(*s);
    }
}

} // namespace kinema::ui::qml
