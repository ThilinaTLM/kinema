// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina.lakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QtTypes>

namespace kinema::core {

/** Inclusive byte interval shared by range-producing policies and consumers. */
struct ByteRange
{
    qint64 start = 0;
    qint64 endInclusive = -1;

    bool isValid() const noexcept { return start >= 0 && endInclusive >= start; }
};

} // namespace kinema::core
