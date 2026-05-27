// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/events/PlaybackEventStream.h"

#include <QSignalSpy>
#include <QTest>
#include <QUuid>

using namespace kinema::playback;
using namespace kinema::playback::events;

class TstPlaybackEventStream : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void publishEmitsEvent()
    {
        PlaybackEventStream stream;
        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(spy.isValid());

        const auto id = QUuid::createUuid();
        PlaybackRequested req;
        req.sessionId = id;
        stream.publish(req);

        QCOMPARE(spy.count(), 1);
        const auto event = spy.first().first().value<PlaybackEvent>();
        QCOMPARE(sessionIdOf(event), id);
        QVERIFY(std::holds_alternative<PlaybackRequested>(event));
    }

    void sessionIdOfWorksForAllAlternatives()
    {
        const auto id = QUuid::createUuid();
        PlaybackEvent e = PositionTicked { id, 12.5 };
        QCOMPARE(sessionIdOf(e), id);

        PlaybackEnded ended;
        ended.sessionId = id;
        ended.reason = PlaybackEndReason::NaturalEof;
        e = ended;
        QCOMPARE(sessionIdOf(e), id);
    }
};

QTEST_MAIN(TstPlaybackEventStream)
#include "tst_playback_event_stream.moc"
