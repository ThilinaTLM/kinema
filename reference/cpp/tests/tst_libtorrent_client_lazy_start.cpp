// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/TorrentStreamingSettings.h"
#include "core/persistence/TorrentCache.h"
#include "domain/PlaybackContext.h"
#include "playback/torrent/LibtorrentClient.h"

#include <KSharedConfig>

#include <QStandardPaths>
#include <QTest>

using namespace kinema;

class TstLibtorrentClientLazyStart : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void constructionIsDormant()
    {
        auto config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-lazy-torrent-test"),
            KConfig::SimpleConfig);
        config::TorrentStreamingSettings settings(config);
        core::TorrentCache cache(settings);

        playback::torrent::LibtorrentClient client(settings, cache);
        QVERIFY2(!client.isStarted(),
            "LibtorrentClient must not construct lt::session in "
            "its constructor");
        QVERIFY(client.session() == nullptr);

        // Settings application is a no-op until the session is
        // built; must not crash.
        client.applyTransferSettings();
        client.setActiveHandleCount(0);
    }

    void dormantMethodsAreNoOps()
    {
        auto config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-lazy-torrent-test"),
            KConfig::SimpleConfig);
        config::TorrentStreamingSettings settings(config);
        core::TorrentCache cache(settings);

        playback::torrent::LibtorrentClient client(settings, cache);

        // None of these should construct the libtorrent session or
        // crash on a dormant instance. They look up entries in an
        // empty session map.
        client.setKeepAlive(QStringLiteral("deadbeef"), true);
        client.pauseInfoHash(QStringLiteral("deadbeef"));
        client.resumeInfoHash(QStringLiteral("deadbeef"));
        client.promoteToFull(QStringLiteral("deadbeef"));
        client.stopInfoHash(QStringLiteral("deadbeef"));

        domain::PlaybackContext ctx;
        ctx.streamRef.infoHash = QStringLiteral("deadbeef");
        client.stopForContext(ctx);

        client.stopAll();

        QVERIFY(client.filesForInfoHash(QStringLiteral("deadbeef"))
                .isEmpty());
        QVERIFY(!client.isStarted());
    }
};

QTEST_GUILESS_MAIN(TstLibtorrentClientLazyStart)

#include "tst_libtorrent_client_lazy_start.moc"
