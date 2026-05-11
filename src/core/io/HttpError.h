// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

#include <stdexcept>

namespace kinema::core {

/**
 * Exception thrown by HttpClient and the higher-level API clients when a
 * request fails. Caught at the UI coroutine boundary and rendered as an
 * error state.
 */
class HttpError : public std::runtime_error
{
public:
    enum class Kind {
        Network, ///< TCP / TLS / DNS / timeout error
        HttpStatus, ///< Server returned a non-2xx HTTP status
        Json, ///< Body could not be parsed as the expected JSON shape
        Cancelled, ///< Request was cancelled before completion
    };

    HttpError(Kind kind, int httpStatus, QString message)
        : std::runtime_error(message.toStdString())
        , m_kind(kind)
        , m_httpStatus(httpStatus)
        , m_message(std::move(message))
    {
    }

    Kind kind() const noexcept { return m_kind; }
    int httpStatus() const noexcept { return m_httpStatus; }
    const QString& message() const noexcept { return m_message; }

private:
    Kind m_kind;
    int m_httpStatus;
    QString m_message;
};

} // namespace kinema::core
