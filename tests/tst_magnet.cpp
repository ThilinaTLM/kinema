// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "core/Magnet.h"

#include <QTest>

using namespace kinema::core::magnet;

class TstMagnet : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void defaultTrackers_hasSix()
    {
        QCOMPARE(defaultTrackers().size(), 6);
        QVERIFY(defaultTrackers().first().startsWith(QLatin1String("udp://")));
    }

    void build_containsInfoHash()
    {
        const auto m = build(QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee"));
        QVERIFY(m.startsWith(QLatin1String("magnet:?xt=urn:btih:aabb1122ccdd3344eeff5566778899aabbccddee")));
    }

    void build_appendsAllDefaultTrackers_inOrder()
    {
        const auto m = build(QStringLiteral("deadbeef"));
        for (const auto& t : defaultTrackers()) {
            // The tracker must appear, URL-encoded, somewhere after the hash.
            const auto encoded = QString::fromUtf8(QUrl::toPercentEncoding(t));
            QVERIFY2(m.contains(encoded),
                qPrintable(QStringLiteral("missing tracker: %1").arg(t)));
        }

        // Trackers must be in declared order.
        int lastPos = -1;
        for (const auto& t : defaultTrackers()) {
            const auto encoded = QString::fromUtf8(QUrl::toPercentEncoding(t));
            const int pos = m.indexOf(encoded);
            QVERIFY(pos > lastPos);
            lastPos = pos;
        }
    }

    void build_encodesDisplayNameSpecialChars()
    {
        const auto m = build(QStringLiteral("deadbeef"),
            QStringLiteral("Foo & Bar+Baz.mkv"));
        // '&' must be escaped so it doesn't open a new query param.
        QVERIFY(m.contains(QLatin1String("&dn=")));
        QVERIFY(m.contains(QLatin1String("%26"))); // the ampersand inside the name
        QVERIFY(!m.contains(QLatin1String("&dn=Foo & Bar")));
    }

    void build_omitsDn_whenEmpty()
    {
        const auto m = build(QStringLiteral("deadbeef"));
        QVERIFY(!m.contains(QLatin1String("&dn=")));
    }
};

QTEST_APPLESS_MAIN(TstMagnet)
#include "tst_magnet.moc"
