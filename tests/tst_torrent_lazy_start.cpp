// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/TorrentStreamingSettings.h"
#include "core/persistence/TorrentCache.h"
#include "domain/PlaybackContext.h"
#include "torrent/TorrentStreamingService.h"

#include <KSharedConfig>

#include <QStandardPaths>
#include <QTest>

using namespace kinema;

class TstTorrentLazyStart : public QObject
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

        torrent::TorrentStreamingService svc(cache, settings);
        QVERIFY2(!svc.isStartedForTests(),
            "TorrentStreamingService must not start libtorrent in "
            "its constructor");
    }

    void dormantMethodsAreNoOps()
    {
        auto config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-lazy-torrent-test"),
            KConfig::SimpleConfig);
        config::TorrentStreamingSettings settings(config);
        core::TorrentCache cache(settings);

        torrent::TorrentStreamingService svc(cache, settings);

        // None of these should construct the libtorrent session or
        // crash on a dormant instance.
        svc.setKeepAlive(QStringLiteral("deadbeef"), true);
        svc.pauseInfoHash(QStringLiteral("deadbeef"));
        svc.resumeInfoHash(QStringLiteral("deadbeef"));
        svc.promoteToFull(QStringLiteral("deadbeef"));
        svc.stopInfoHash(QStringLiteral("deadbeef"));

        domain::PlaybackContext ctx;
        ctx.streamRef.infoHash = QStringLiteral("deadbeef");
        svc.stopForContext(ctx);

        svc.stopAll();

        QVERIFY(svc.filesForInfoHash(QStringLiteral("deadbeef")).isEmpty());
        QVERIFY(!svc.isStartedForTests());
    }
};

QTEST_GUILESS_MAIN(TstTorrentLazyStart)

#include "tst_torrent_lazy_start.moc"
