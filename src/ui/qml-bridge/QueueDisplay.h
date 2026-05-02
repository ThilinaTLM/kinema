// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/QueueItem.h"

#include <QString>
#include <QStringList>

namespace kinema::ui::qml::queue_display {

QString previewTitle(const api::QueueItem& item);
QString subtitle(const api::QueueItem& item);
QStringList previewChips(const api::QueueItem& item);

} // namespace kinema::ui::qml::queue_display
