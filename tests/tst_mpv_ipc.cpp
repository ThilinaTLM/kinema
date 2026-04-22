// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MpvIpc.h"

#include <mpv/client.h>

#include <QByteArray>
#include <QList>
#include <QSignalSpy>
#include <QTest>
#include <QtGlobal>

#include <cstring>

using namespace kinema::core;

namespace {

/// Build a synthetic `mpv_event_client_message` around a fixed list
/// of UTF-8 byte arrays. The arrays remain owned by the caller; the
/// returned `Payload` holds on to pointer buffers so the struct
/// stays valid for the duration of a single `deliver()` call.
struct Payload {
    QList<QByteArray> args;
    QList<const char*> ptrs;
    mpv_event_client_message msg {};
};

Payload makePayload(std::initializer_list<const char*> args)
{
    Payload p;
    p.args.reserve(static_cast<int>(args.size()));
    p.ptrs.reserve(static_cast<int>(args.size()));
    for (const char* a : args) {
        p.args.append(QByteArray(a));
    }
    for (const auto& a : p.args) {
        p.ptrs.append(a.constData());
    }
    p.msg.num_args = p.ptrs.size();
    // libmpv exposes `args` as `const char**`; `QList::data()`
    // hands us `const char* const*` (the list's storage is
    // mutable, the pointees are const). The cast strips the inner
    // `const` to match the header. We never mutate through these
    // pointers.
    p.msg.args = const_cast<const char**>(p.ptrs.data());
    return p;
}

} // namespace

class TestMpvIpc : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    // Simple no-argument commands round-trip one signal apiece.
    void simpleCommands_data()
    {
        QTest::addColumn<QByteArray>("cmd");
        QTest::addColumn<QByteArray>("signalName");

        QTest::newRow("resume-accepted")
            << QByteArrayLiteral("resume-accepted")
            << QByteArrayLiteral("resumeAccepted()");
        QTest::newRow("resume-declined")
            << QByteArrayLiteral("resume-declined")
            << QByteArrayLiteral("resumeDeclined()");
        QTest::newRow("next-episode-accepted")
            << QByteArrayLiteral("next-episode-accepted")
            << QByteArrayLiteral("nextEpisodeAccepted()");
        QTest::newRow("next-episode-cancelled")
            << QByteArrayLiteral("next-episode-cancelled")
            << QByteArrayLiteral("nextEpisodeCancelled()");
        QTest::newRow("skip-requested")
            << QByteArrayLiteral("skip-requested")
            << QByteArrayLiteral("skipRequested()");
        QTest::newRow("close-player")
            << QByteArrayLiteral("close-player")
            << QByteArrayLiteral("closeRequested()");
        QTest::newRow("toggle-fullscreen")
            << QByteArrayLiteral("toggle-fullscreen")
            << QByteArrayLiteral("fullscreenToggleRequested()");
    }

    void simpleCommands()
    {
        QFETCH(QByteArray, cmd);
        QFETCH(QByteArray, signalName);

        MpvIpc ipc(/*mpv=*/nullptr, this);
        // `QSIGNAL` expands to the SIGNAL() macro's prefix "2" then
        // the normalised signature. We build the string manually
        // for the data-driven rows. `QT_USE_QSTRINGBUILDER` turns
        // QByteArray + QByteArray into a deferred concatenation;
        // force the conversion with an explicit QByteArray{…}.
        QByteArray spec = QByteArrayLiteral("2");
        spec += signalName;
        QSignalSpy spy(&ipc, spec.constData());
        QVERIFY(spy.isValid());

        auto payload = makePayload({ cmd.constData() });
        ipc.deliver(&payload.msg);

        QCOMPARE(spy.count(), 1);
    }

    // pick-audio / pick-subtitle / pick-speed translate their arg
    // through the same int / double parser the old overlay used.
    void audioPickedParsesInt()
    {
        MpvIpc ipc(nullptr, this);
        QSignalSpy spy(&ipc, SIGNAL(audioPicked(int)));

        auto ok = makePayload({ "pick-audio", "2" });
        ipc.deliver(&ok.msg);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.takeFirst().at(0).toInt(), 2);

        auto none = makePayload({ "pick-audio", "no" });
        ipc.deliver(&none.msg);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.takeFirst().at(0).toInt(), -1);

        auto garbage = makePayload({ "pick-audio", "zzzz" });
        ipc.deliver(&garbage.msg);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.takeFirst().at(0).toInt(), -1);
    }

    void subtitlePickedParsesInt()
    {
        MpvIpc ipc(nullptr, this);
        QSignalSpy spy(&ipc, SIGNAL(subtitlePicked(int)));

        auto ok = makePayload({ "pick-subtitle", "1" });
        ipc.deliver(&ok.msg);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.takeFirst().at(0).toInt(), 1);
    }

    void speedPickedParsesDouble()
    {
        MpvIpc ipc(nullptr, this);
        QSignalSpy spy(&ipc, SIGNAL(speedPicked(double)));

        auto ok = makePayload({ "pick-speed", "1.5" });
        ipc.deliver(&ok.msg);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.takeFirst().at(0).toDouble(), 1.5);

        // Non-numeric payload is ignored; no signal is emitted.
        auto bad = makePayload({ "pick-speed", "nope" });
        ipc.deliver(&bad.msg);
        QCOMPARE(spy.count(), 0);
    }

    // Unknown commands must not produce any signal emission on the
    // "typed" surface. The warning on stderr is fine.
    void unknownCommandIgnored()
    {
        MpvIpc ipc(nullptr, this);
        QSignalSpy spyA(&ipc, SIGNAL(audioPicked(int)));
        QSignalSpy spyS(&ipc, SIGNAL(skipRequested()));
        QSignalSpy spyC(&ipc, SIGNAL(closeRequested()));

        auto payload = makePayload({ "foo-bar", "1", "2" });
        ipc.deliver(&payload.msg);

        QCOMPARE(spyA.count(), 0);
        QCOMPARE(spyS.count(), 0);
        QCOMPARE(spyC.count(), 0);
    }

    // A null or empty message must never dereference past the
    // zero-arg guard.
    void nullAndEmptyMessagesAreSafe()
    {
        MpvIpc ipc(nullptr, this);
        QSignalSpy spy(&ipc, SIGNAL(skipRequested()));

        ipc.deliver(nullptr);
        QCOMPARE(spy.count(), 0);

        mpv_event_client_message empty {};
        empty.num_args = 0;
        empty.args = nullptr;
        ipc.deliver(&empty);
        QCOMPARE(spy.count(), 0);
    }
};

QTEST_GUILESS_MAIN(TestMpvIpc)
#include "tst_mpv_ipc.moc"
