// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/TransferStateMachine.h"

#include <QTest>

using namespace kinema::playback::session;

class TstTransferStateMachine : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void onDemandHappyPath()
    {
        TransferStateMachine sm;
        QCOMPARE(sm.handle(TransferInput::OpenSession),
            TransferState::Queued);
        QCOMPARE(sm.handle(TransferInput::OnDemandReady),
            TransferState::Streaming);
        QCOMPARE(sm.handle(TransferInput::AllBytesCached),
            TransferState::Complete);
    }

    void fullDownloadPath()
    {
        TransferStateMachine sm;
        sm.handle(TransferInput::OpenSession);
        QCOMPARE(sm.handle(TransferInput::FullReady),
            TransferState::Downloading);
    }

    void upgradeOnDemandToFull()
    {
        TransferStateMachine sm;
        sm.handle(TransferInput::OpenSession);
        sm.handle(TransferInput::OnDemandReady);
        QCOMPARE(sm.handle(TransferInput::SaveOffline),
            TransferState::Downloading);
    }

    void playerDetachIdlesOnDemand()
    {
        TransferStateMachine sm;
        sm.handle(TransferInput::OpenSession);
        sm.handle(TransferInput::OnDemandReady);
        QCOMPARE(sm.handle(TransferInput::PlayerDetached),
            TransferState::IdleCached);
        QCOMPARE(sm.handle(TransferInput::PlayerAttached),
            TransferState::Streaming);
    }

    void errorMovesToFailed()
    {
        TransferStateMachine sm;
        sm.handle(TransferInput::OpenSession);
        sm.handle(TransferInput::Error);
        QCOMPARE(sm.state(), TransferState::Failed);
        QCOMPARE(sm.handle(TransferInput::Retry),
            TransferState::Queued);
    }

    void downloadStateProjectionMatches()
    {
        TransferStateMachine sm;
        QCOMPARE(sm.toDownloadState(false),
            kinema::domain::DownloadState::Queued);
        sm.handle(TransferInput::OpenSession);
        sm.handle(TransferInput::OnDemandReady);
        QCOMPARE(sm.toDownloadState(true),
            kinema::domain::DownloadState::Active);
        sm.handle(TransferInput::PlayerDetached);
        QCOMPARE(sm.toDownloadState(false),
            kinema::domain::DownloadState::Idle);
        sm.handle(TransferInput::AllBytesCached);
        QCOMPARE(sm.toDownloadState(false),
            kinema::domain::DownloadState::Completed);
    }

    void removeIsTerminal()
    {
        TransferStateMachine sm;
        sm.handle(TransferInput::OpenSession);
        sm.handle(TransferInput::Remove);
        QCOMPARE(sm.state(), TransferState::Removed);
        // Subsequent inputs ignored.
        QCOMPARE(sm.handle(TransferInput::Retry),
            TransferState::Removed);
    }
};

QTEST_MAIN(TstTransferStateMachine)
#include "tst_transfer_state_machine.moc"
