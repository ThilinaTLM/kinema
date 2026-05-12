// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/mpv/MprisMetadata.h"

#include <QDBusObjectPath>
#include <QtTest>

using kinema::domain::MediaKind;
using kinema::domain::PlaybackContext;
using kinema::domain::PlaybackKey;

class TestMprisMetadata : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void movieMetadata();
    void episodicMetadata();
    void trackPathDeterministic();
};

void TestMprisMetadata::movieMetadata()
{
    PlaybackContext ctx;
    ctx.key.kind = MediaKind::Movie;
    ctx.key.imdbId = QStringLiteral("tt1234567");
    ctx.title = QStringLiteral("Arrival");
    ctx.poster = QUrl(QStringLiteral("https://example.com/poster.jpg"));

    const auto map = kinema::core::mpris::metadata(ctx, 7265.0);
    QCOMPARE(map.value(QStringLiteral("xesam:title")).toString(),
        QStringLiteral("Arrival"));
    QCOMPARE(map.value(QStringLiteral("mpris:artUrl")).toString(),
        QStringLiteral("https://example.com/poster.jpg"));
    QCOMPARE(map.value(QStringLiteral("mpris:length")).toLongLong(),
        7265000000LL);

    const auto trackId = qvariant_cast<QDBusObjectPath>(
        map.value(QStringLiteral("mpris:trackid")));
    QVERIFY(trackId.path().startsWith(QStringLiteral("/dev/tlmtech/kinema/track/")));
}

void TestMprisMetadata::episodicMetadata()
{
    PlaybackContext ctx;
    ctx.key.kind = MediaKind::Series;
    ctx.key.imdbId = QStringLiteral("tt7654321");
    ctx.key.season = 1;
    ctx.key.episode = 2;
    ctx.title = QStringLiteral("Pilot");
    ctx.seriesTitle = QStringLiteral("Example Show");

    const auto map = kinema::core::mpris::metadata(ctx, -1.0);
    QCOMPARE(map.value(QStringLiteral("xesam:title")).toString(),
        QStringLiteral("Pilot"));
    QCOMPARE(map.value(QStringLiteral("xesam:album")).toString(),
        QStringLiteral("Example Show"));
    QVERIFY(!map.contains(QStringLiteral("mpris:length")));
}

void TestMprisMetadata::trackPathDeterministic()
{
    PlaybackKey key;
    key.kind = MediaKind::Series;
    key.imdbId = QStringLiteral("tt7654321");
    key.season = 1;
    key.episode = 2;

    const QString a = kinema::core::mpris::trackObjectPath(key);
    const QString b = kinema::core::mpris::trackObjectPath(key);
    QCOMPARE(a, b);
    QVERIFY(a.startsWith(QStringLiteral("/dev/tlmtech/kinema/track/")));
    QVERIFY(!a.contains(QLatin1Char(':')));
}

QTEST_GUILESS_MAIN(TestMprisMetadata)
#include "tst_mpris_metadata.moc"
