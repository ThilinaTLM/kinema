// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/HttpErrorPresenter.h"

#include "core/HttpError.h"
#include "kinema_debug.h"

namespace kinema::core {

QString describeError(const std::exception& e, const char* context)
{
    if (const auto* he = dynamic_cast<const HttpError*>(&e)) {
        qCWarning(KINEMA).nospace()
            << context << ": " << he->message()
            << " (status=" << he->httpStatus() << ")";
        return he->message();
    }
    const auto msg = QString::fromUtf8(e.what());
    qCWarning(KINEMA).nospace() << context << " (unknown): " << msg;
    return msg;
}

} // namespace kinema::core
