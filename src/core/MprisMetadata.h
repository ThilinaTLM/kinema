// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QVariantMap>

namespace kinema::core::mpris {

QString trackObjectPath(const api::PlaybackKey& key);
QVariantMap metadata(const api::PlaybackContext& ctx,
    double durationSeconds);

} // namespace kinema::core::mpris
