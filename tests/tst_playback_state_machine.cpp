// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/PlaybackStateMachine.h"

#include <QTest>

using namespace kinema::playback::session;

class TstPlaybackStateMachine : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initialState()
    {
        PlaybackStateMachine sm;
        QCOMPARE(sm.state(), PlaybackState::Idle);
    }

    void happyPath()
    {
        PlaybackStateMachine sm;
        QCOMPARE(sm.handle(PlaybackInput::PlayStream),
            PlaybackState::ResolvingSource);
        QCOMPARE(sm.handle(PlaybackInput::SourceResolved),
            PlaybackState::PreparingTransfer);
        QCOMPARE(sm.handle(PlaybackInput::PlayableUrlReady),
            PlaybackState::LoadingPlayer);
        QCOMPARE(sm.handle(PlaybackInput::PlayerLoaded),
            PlaybackState::Playing);
        QCOMPARE(sm.handle(PlaybackInput::Pause),
            PlaybackState::Paused);
        QCOMPARE(sm.handle(PlaybackInput::Resume),
            PlaybackState::Playing);
        QCOMPARE(sm.handle(PlaybackInput::EndOfFile),
            PlaybackState::Ending);
        // Ending -> Completed on next observation
        QCOMPARE(sm.handle(PlaybackInput::PlayerLoaded),
            PlaybackState::Completed);
    }

    void resumePromptPath()
    {
        PlaybackStateMachine sm;
        sm.handle(PlaybackInput::PlayStream);
        sm.handle(PlaybackInput::SourceResolved);
        sm.handle(PlaybackInput::PlayableUrlReady);
        QCOMPARE(sm.handle(PlaybackInput::ResumePromptRequired),
            PlaybackState::ShowingResumePrompt);
        QCOMPARE(sm.handle(PlaybackInput::ResumeAccepted),
            PlaybackState::Playing);
    }

    void replacingSourceTransitionsBackToResolving()
    {
        PlaybackStateMachine sm;
        sm.handle(PlaybackInput::PlayStream);
        sm.handle(PlaybackInput::SourceResolved);
        sm.handle(PlaybackInput::PlayableUrlReady);
        sm.handle(PlaybackInput::PlayerLoaded);
        QCOMPARE(sm.state(), PlaybackState::Playing);
        // Fresh PlayStream should rewind to ResolvingSource.
        QCOMPARE(sm.handle(PlaybackInput::PlayStream),
            PlaybackState::ResolvingSource);
    }

    void errorMovesToFailed()
    {
        PlaybackStateMachine sm;
        sm.handle(PlaybackInput::PlayStream);
        QCOMPARE(sm.handle(PlaybackInput::Error),
            PlaybackState::Failed);
        // Terminal: subsequent inputs ignored.
        QCOMPARE(sm.handle(PlaybackInput::PlayerLoaded),
            PlaybackState::Failed);
    }

    void loadTimeoutMovesToFailed()
    {
        PlaybackStateMachine sm;
        sm.handle(PlaybackInput::PlayStream);
        sm.handle(PlaybackInput::SourceResolved);
        sm.handle(PlaybackInput::PlayableUrlReady);
        QCOMPARE(sm.handle(PlaybackInput::LoadTimeout),
            PlaybackState::Failed);
    }

    void stopFromPlayingMovesToStopping()
    {
        PlaybackStateMachine sm;
        sm.handle(PlaybackInput::PlayStream);
        sm.handle(PlaybackInput::SourceResolved);
        sm.handle(PlaybackInput::PlayableUrlReady);
        sm.handle(PlaybackInput::PlayerLoaded);
        QCOMPARE(sm.handle(PlaybackInput::Stop),
            PlaybackState::Stopping);
    }

    void resetClearsState()
    {
        PlaybackStateMachine sm;
        sm.handle(PlaybackInput::PlayStream);
        sm.handle(PlaybackInput::Error);
        QCOMPARE(sm.state(), PlaybackState::Failed);
        sm.reset();
        QCOMPARE(sm.state(), PlaybackState::Idle);
    }
};

QTEST_MAIN(TstPlaybackStateMachine)
#include "tst_playback_state_machine.moc"
