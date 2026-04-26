// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Moviehash.h"

#include <QByteArray>
#include <QTest>

using namespace kinema::core::moviehash;

namespace {

constexpr qint64 kBlock = 65536;

QByteArray zeroBlock()
{
    return QByteArray(static_cast<int>(kBlock), '\0');
}

} // namespace

class TstMoviehash : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testZeroBlocksHashEqualsSize()
    {
        // Two 64 KB zero blocks → sums add nothing, hash == size.
        const qint64 size = 600 * 1024 * 1024; // 600 MB
        const auto hex = compute(zeroBlock(), zeroBlock(), size);
        QCOMPARE(hex.size(), 16);
        bool ok = false;
        const quint64 v = hex.toULongLong(&ok, 16);
        QVERIFY(ok);
        QCOMPARE(v, static_cast<quint64>(size));
    }

    void testKnownVector()
    {
        // Construct a deterministic vector: head full of 0x01 bytes,
        // tail full of 0x02 bytes. Each block has 65536/8 = 8192
        // 8-byte words.
        QByteArray head(static_cast<int>(kBlock), '\x01');
        QByteArray tail(static_cast<int>(kBlock), '\x02');
        const qint64 size = 1'000'000;

        // Each 8-byte LE word of the head: 0x0101010101010101.
        constexpr quint64 wordHead = 0x0101010101010101ULL;
        constexpr quint64 wordTail = 0x0202020202020202ULL;
        constexpr quint64 wordCount = kBlock / 8;
        const quint64 expected = static_cast<quint64>(size)
            + wordCount * wordHead + wordCount * wordTail;

        const auto hex = compute(head, tail, size);
        bool ok = false;
        const quint64 actual = hex.toULongLong(&ok, 16);
        QVERIFY(ok);
        QCOMPARE(actual, expected);
        QCOMPARE(hex.size(), 16);
    }

    void testInvalidInputs()
    {
        QVERIFY(compute(QByteArray(100, '\0'), zeroBlock(), 100).isEmpty());
        QVERIFY(compute(zeroBlock(), QByteArray(100, '\0'), 100).isEmpty());
        QVERIFY(compute(zeroBlock(), zeroBlock(), 0).isEmpty());
    }

    void testHexIsLowercaseAndPadded()
    {
        // Pick a size that produces a small number with leading zeros.
        const auto hex = compute(zeroBlock(), zeroBlock(), 1);
        QCOMPARE(hex, QStringLiteral("0000000000000001"));
    }
};

QTEST_MAIN(TstMoviehash)
#include "tst_moviehash.moc"
