// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Moviehash.h"

#include <cstdint>
#include <cstring>

namespace kinema::core::moviehash {

namespace {

constexpr qint64 kBlock = 65536;

quint64 sumBlock(const QByteArray& block)
{
    quint64 sum = 0;
    const auto* data = reinterpret_cast<const unsigned char*>(block.constData());
    for (qint64 i = 0; i + 8 <= block.size(); i += 8) {
        quint64 word = 0;
        std::memcpy(&word, data + i, 8);
        // memcpy of an LE-host word produces the right value already
        // on every platform Kinema targets; if we ever ship on a
        // big-endian host this needs a byteswap.
        sum += word;
    }
    return sum;
}

} // namespace

QString compute(const QByteArray& head,
    const QByteArray& tail, qint64 totalSize)
{
    if (head.size() != kBlock || tail.size() != kBlock || totalSize <= 0) {
        return {};
    }
    quint64 hash = static_cast<quint64>(totalSize);
    hash += sumBlock(head);
    hash += sumBlock(tail);

    // 16-char lowercase hex, padded with leading zeros.
    QString out = QString::number(hash, 16);
    while (out.size() < 16) {
        out.prepend(QLatin1Char('0'));
    }
    return out;
}

} // namespace kinema::core::moviehash
