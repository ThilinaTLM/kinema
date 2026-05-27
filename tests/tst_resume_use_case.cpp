// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/resume/ResumeUseCase.h"

#include "core/persistence/Database.h"
#include "core/persistence/HistoryStore.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/history/HistoryQueryService.h"
#include "playback/history/SqlitePlaybackHistoryRepository.h"
#include "playback/ports/PlaybackHistoryRepository.h"
#include "playback/ports/StreamIndexerPort.h"
#include "playback/progress/PlaybackProgressProjector.h"
#include "services/StreamActions.h"

#include <QCoreApplication>
#include <QDateTime>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

#include <memory>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::resume;

namespace {

class StubIndexer final : public ports::StreamIndexerPort
{
public:
    QList<domain::Stream> next;
    bool throwOnNext = false;
    int calls = 0;

    QCoro::Task<QList<domain::Stream>> streamsFor(
        domain::MediaKind, const QString&) override
    {
        ++calls;
        if (throwOnNext) {
            throw std::runtime_error("indexer offline");
        }
        co_return next;
    }
};

class RecordingActions : public services::StreamActions
{
public:
    explicit RecordingActions(QObject* parent = nullptr)
        : services::StreamActions(/*launcher=*/nullptr, parent)
    {
    }

    struct Call {
        domain::Stream stream;
        domain::PlaybackContext ctx;
    };
    QList<Call> calls;

    void play(const domain::Stream& s,
        const domain::PlaybackContext& ctx) override
    {
        calls.append({ s, ctx });
    }
};

domain::HistoryEntry makeEntry(const QString& imdb,
    const QString& release, const QString& hash)
{
    domain::HistoryEntry e;
    e.key.kind = domain::MediaKind::Movie;
    e.key.imdbId = imdb;
    e.title = QStringLiteral("Movie ") + imdb;
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();
    e.lastStream.infoHash = hash;
    e.lastStream.releaseName = release;
    e.lastStream.resolution = QStringLiteral("1080p");
    return e;
}

domain::Stream makeStream(const QString& release, const QString& hash,
    const QString& directUrl = {})
{
    domain::Stream s;
    s.releaseName = release;
    s.infoHash = hash;
    s.qualityLabel = QStringLiteral("Torrentio 1080p");
    s.resolution = QStringLiteral("1080p");
    s.directUrl = QUrl(directUrl);
    return s;
}

void drain()
{
    for (int i = 0; i < 8; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstResumeUseCase : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        m_tmp = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmp->isValid());
        m_db = std::make_unique<core::Database>(
            m_tmp->filePath(QStringLiteral("kinema.db")), nullptr);
        QVERIFY(m_db->open());
        m_history = std::make_unique<core::HistoryStore>(*m_db);
        m_repo = std::make_unique<history::SqlitePlaybackHistoryRepository>(
            *m_history);
        m_query = std::make_unique<history::HistoryQueryService>(*m_repo,
            *m_history);
        m_eventStream = std::make_unique<events::PlaybackEventStream>();
        m_projector = std::make_unique<progress::PlaybackProgressProjector>(
            *m_repo, *m_eventStream);
        m_indexer = std::make_unique<StubIndexer>();
        m_actions = std::make_unique<RecordingActions>();
        m_useCase = std::make_unique<ResumeUseCase>(*m_query, *m_projector,
            *m_indexer, *m_repo, *m_actions);
    }

    void cleanup()
    {
        m_useCase.reset();
        m_actions.reset();
        m_indexer.reset();
        m_projector.reset();
        m_eventStream.reset();
        m_query.reset();
        m_repo.reset();
        m_history.reset();
        m_db.reset();
        m_tmp.reset();
    }

    // -------- resumeSecondsFor --------

    void resumeSecondsFromStoredRow()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QStringLiteral("R"),
            QStringLiteral("h"));
        e.positionSec = 1200;
        e.durationSec = 7200;
        m_repo->record(e);

        const auto r = m_useCase->resumeSecondsFor(e.key);
        QVERIFY(r.has_value());
        QCOMPARE(*r, 1200);
    }

    void resumeSecondsPrefersProjectorLivePositionOnSameKey()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QStringLiteral("R"),
            QStringLiteral("h"));
        e.positionSec = 120;
        e.durationSec = 7200;
        m_repo->record(e);

        // Drive the projector to track this key with a fresher live
        // position than the disk row.
        const auto sessionId = QUuid::createUuid();
        domain::PlaybackContext ctx;
        ctx.key = e.key;
        m_eventStream->publish(events::PlaybackRequested {
            sessionId, ctx });
        m_eventStream->publish(events::DurationChanged {
            sessionId, 7200.0 });
        // Push the projector's lastPosition above the disk row.
        // The projector's persist-throttle is irrelevant; we only
        // care about its in-memory lastPosition().
        m_eventStream->publish(events::PositionTicked {
            sessionId, 900.0 });

        const auto r = m_useCase->resumeSecondsFor(e.key);
        QVERIFY(r.has_value());
        QCOMPARE(*r, 900);
    }

    void resumeSecondsIgnoresFinishedRow()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QStringLiteral("R"),
            QStringLiteral("h"));
        e.positionSec = 6000;
        e.durationSec = 6000;
        e.finished = true;
        m_repo->record(e);

        QCOMPARE(m_useCase->resumeSecondsFor(e.key), std::nullopt);
    }

    // -------- removeEntry --------

    void removeEntryClearsRepoRow()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QStringLiteral("R"),
            QStringLiteral("h"));
        m_repo->record(e);
        QVERIFY(m_repo->find(e.key).has_value());

        m_useCase->removeEntry(e);
        QVERIFY(!m_repo->find(e.key).has_value());
    }

    // -------- resume() coroutine --------

    void resumeDispatchesMatchedStream()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QStringLiteral("Release.X"),
            QStringLiteral("hashX"));

        m_indexer->next.append(makeStream(QStringLiteral("Release.Y"),
            QStringLiteral("hashY")));
        m_indexer->next.append(makeStream(QStringLiteral("Release.X"),
            QStringLiteral("hashX"),
            QStringLiteral("http://x/direct.mkv")));

        QSignalSpy statusSpy(m_useCase.get(),
            &ResumeUseCase::statusMessage);
        QSignalSpy fallbackSpy(m_useCase.get(),
            &ResumeUseCase::resumeFallbackRequested);

        m_useCase->resume(e);
        drain();

        QCOMPARE(m_actions->calls.size(), 1);
        QCOMPARE(m_actions->calls[0].stream.infoHash,
            QStringLiteral("hashX"));
        QCOMPARE(m_actions->calls[0].stream.directUrl.toString(),
            QStringLiteral("http://x/direct.mkv"));
        QCOMPARE(fallbackSpy.count(), 0);
        // An initial "Resuming..." status is emitted.
        QVERIFY(statusSpy.count() >= 1);
    }

    void resumeMissingReleaseFiresFallback()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QStringLiteral("Release.X"),
            QStringLiteral("hashX"));
        m_indexer->next.append(makeStream(QStringLiteral("Release.Y"),
            QStringLiteral("hashY")));

        QSignalSpy fallbackSpy(m_useCase.get(),
            &ResumeUseCase::resumeFallbackRequested);
        m_useCase->resume(e);
        drain();

        QCOMPARE(m_actions->calls.size(), 0);
        QCOMPARE(fallbackSpy.count(), 1);
    }

    void resumeWithoutSavedReleaseFiresFallback()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QString {}, QString {});

        QSignalSpy fallbackSpy(m_useCase.get(),
            &ResumeUseCase::resumeFallbackRequested);
        m_useCase->resume(e);
        drain();

        // Empty release reference is rejected synchronously; the
        // indexer is never queried.
        QCOMPARE(m_indexer->calls, 0);
        QCOMPARE(m_actions->calls.size(), 0);
        QCOMPARE(fallbackSpy.count(), 1);
    }

    void resumeIndexerFailureFiresFallback()
    {
        auto e = makeEntry(QStringLiteral("tt1"), QStringLiteral("Release.X"),
            QStringLiteral("hashX"));
        m_indexer->throwOnNext = true;

        QSignalSpy fallbackSpy(m_useCase.get(),
            &ResumeUseCase::resumeFallbackRequested);
        m_useCase->resume(e);
        drain();

        QCOMPARE(m_actions->calls.size(), 0);
        QCOMPARE(fallbackSpy.count(), 1);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_history;
    std::unique_ptr<history::SqlitePlaybackHistoryRepository> m_repo;
    std::unique_ptr<history::HistoryQueryService> m_query;
    std::unique_ptr<events::PlaybackEventStream> m_eventStream;
    std::unique_ptr<progress::PlaybackProgressProjector> m_projector;
    std::unique_ptr<StubIndexer> m_indexer;
    std::unique_ptr<RecordingActions> m_actions;
    std::unique_ptr<ResumeUseCase> m_useCase;
};

QTEST_MAIN(TstResumeUseCase)
#include "tst_resume_use_case.moc"
