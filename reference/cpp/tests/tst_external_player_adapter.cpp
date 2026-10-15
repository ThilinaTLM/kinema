// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/adapters/ExternalPlayerAdapter.h"

#include "config/AppSettings.h"
#include "config/PlayerSettings.h"
#include "core/mpv/Player.h"
#include "core/mpv/PlayerLauncher.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"

#include <KConfigGroup>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QStandardPaths>
#include <QTest>
#include <QUuid>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::adapters;
using namespace kinema::playback::events;

namespace {

domain::PlaybackContext makeCtx()
{
    domain::PlaybackContext ctx;
    domain::PlaybackKey key;
    key.kind = domain::MediaKind::Movie;
    key.imdbId = QStringLiteral("tt0111161");
    ctx.key = key;
    ctx.title = QStringLiteral("The Shawshank Redemption");
    return ctx;
}

PlaybackEvent firstEventOf(QSignalSpy& spy, qsizetype index = 0)
{
    return spy.at(index).first().value<PlaybackEvent>();
}

} // namespace

class TstExternalPlayerAdapter : public QObject
{
    Q_OBJECT

private:
    PlaybackEventStream m_stream;
    config::AppSettings m_appSettings;
    core::PlayerLauncher m_launcher { m_appSettings.player() };

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void publishesPlayerLoadedOnLaunched()
    {
        ExternalPlayerAdapter adapter(m_launcher, m_stream);
        const auto sessionId = QUuid::createUuid();
        adapter.setActiveSession(sessionId, makeCtx());

        QSignalSpy spy(&m_stream, &PlaybackEventStream::eventPublished);
        Q_EMIT m_launcher.launched(core::player::Kind::Mpv,
            QStringLiteral("foo"));

        QCOMPARE(spy.count(), 1);
        const auto event = firstEventOf(spy);
        QVERIFY(std::holds_alternative<PlayerLoaded>(event));
        QCOMPARE(sessionIdOf(event), sessionId);
    }

    void publishesPlaybackFailedOnLaunchFailed()
    {
        ExternalPlayerAdapter adapter(m_launcher, m_stream);
        const auto sessionId = QUuid::createUuid();
        adapter.setActiveSession(sessionId, makeCtx());

        QSignalSpy spy(&m_stream, &PlaybackEventStream::eventPublished);
        Q_EMIT m_launcher.launchFailed(core::player::Kind::Mpv,
            QStringLiteral("mpv not on PATH"));

        QCOMPARE(spy.count(), 1);
        const auto event = firstEventOf(spy);
        QVERIFY(std::holds_alternative<PlaybackFailed>(event));
        const auto& failed = std::get<PlaybackFailed>(event);
        QCOMPARE(failed.sessionId, sessionId);
        QCOMPARE(failed.reason, QStringLiteral("mpv not on PATH"));
    }

    void ignoresLaunchEventsWhenNoActiveSession()
    {
        ExternalPlayerAdapter adapter(m_launcher, m_stream);
        // No setActiveSession call: launches are stale.
        QSignalSpy spy(&m_stream, &PlaybackEventStream::eventPublished);
        Q_EMIT m_launcher.launched(core::player::Kind::Mpv,
            QStringLiteral("x"));
        Q_EMIT m_launcher.launchFailed(core::player::Kind::Mpv,
            QStringLiteral("y"));
        QCOMPARE(spy.count(), 0);
    }

    void stopPublishesUserStop()
    {
        ExternalPlayerAdapter adapter(m_launcher, m_stream);
        const auto sessionId = QUuid::createUuid();
        adapter.setActiveSession(sessionId, makeCtx());

        QSignalSpy spy(&m_stream, &PlaybackEventStream::eventPublished);
        adapter.stop();

        QCOMPARE(spy.count(), 1);
        const auto event = firstEventOf(spy);
        QVERIFY(std::holds_alternative<PlaybackEnded>(event));
        const auto& ended = std::get<PlaybackEnded>(event);
        QCOMPARE(ended.reason, PlaybackEndReason::UserStop);
        QCOMPARE(ended.sessionId, sessionId);
    }
};

QTEST_MAIN(TstExternalPlayerAdapter)
#include "tst_external_player_adapter.moc"
