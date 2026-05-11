// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/PlaybackContext.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "controllers/WatchedController.h"
#include "core/persistence/Database.h"
#include "core/persistence/HistoryStore.h"
#include "core/persistence/WatchedStore.h"

#include <KSharedConfig>

#include <QDateTime>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

namespace {

domain::HistoryEntry makeFinishedMovieEntry(const QString& imdb)
{
    domain::HistoryEntry e;
    e.key.kind = domain::MediaKind::Movie;
    e.key.imdbId = imdb;
    e.title = QStringLiteral("Movie ") + imdb;
    e.positionSec = 6000;
    e.durationSec = 6000;
    e.finished = true;
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();
    return e;
}

domain::HistoryEntry makeInProgressEpisodeEntry(const QString& imdb,
    int season, int episode, double pos, double dur)
{
    domain::HistoryEntry e;
    e.key.kind = domain::MediaKind::Series;
    e.key.imdbId = imdb;
    e.key.season = season;
    e.key.episode = episode;
    e.title = QStringLiteral("Episode");
    e.positionSec = pos;
    e.durationSec = dur;
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();
    return e;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstWatchedController : public QObject
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
        m_watchedStore = std::make_unique<core::WatchedStore>(*m_db);
        m_config = KSharedConfig::openConfig(
            m_tmp->filePath(QStringLiteral("kinemarc")));
        m_settings = std::make_unique<config::AppSettings>(m_config, nullptr);
        m_historyCtrl = std::make_unique<controllers::HistoryController>(
            *m_history, /*indexers=*/nullptr, m_emptyToken);
        m_watchedCtrl = std::make_unique<controllers::WatchedController>(
            *m_watchedStore, m_historyCtrl.get());
    }

    void cleanup()
    {
        m_watchedCtrl.reset();
        m_historyCtrl.reset();
        m_settings.reset();
        m_config.reset();
        m_watchedStore.reset();
        m_history.reset();
        m_db.reset();
        m_tmp.reset();
    }

    // ---- override precedence ------------------------------------------

    void watchedOverrideWinsWithoutHistory()
    {
        m_watchedCtrl->setMovieWatched(QStringLiteral("tt1"), true);
        QVERIFY(m_watchedCtrl->isMovieWatched(QStringLiteral("tt1")));
    }

    void unwatchedOverrideOverridesFinishedHistory()
    {
        m_history->record(makeFinishedMovieEntry(QStringLiteral("tt1")));
        QVERIFY(m_watchedCtrl->isMovieWatched(QStringLiteral("tt1")));

        // Override Unwatched should win even though history.finished = 1.
        m_watchedCtrl->setMovieWatched(QStringLiteral("tt1"), false);
        QVERIFY(!m_watchedCtrl->isMovieWatched(QStringLiteral("tt1")));
    }

    void historyFinishedFallsThroughWithNoOverride()
    {
        m_history->record(makeFinishedMovieEntry(QStringLiteral("tt2")));
        QVERIFY(m_watchedCtrl->isMovieWatched(QStringLiteral("tt2")));
    }

    // ---- progress + resume --------------------------------------------

    void progressAndResumeForInProgressEpisode()
    {
        const QString imdb = QStringLiteral("ttSeries");
        m_history->record(makeInProgressEpisodeEntry(imdb, 1, 1, 600, 6000));
        QVERIFY(!m_watchedCtrl->isEpisodeWatched(imdb, 1, 1));
        QVERIFY(m_watchedCtrl->episodeProgress(imdb, 1, 1) > 0.0);
        const auto resume = m_watchedCtrl->resumeEntryForEpisode(imdb, 1, 1);
        QVERIFY(resume.has_value());
        QCOMPARE(resume->positionSec, 600.0);
    }

    // ---- bulk season / series mark ------------------------------------

    void setEpisodesWatchedFlipsAllRows()
    {
        const QString imdb = QStringLiteral("ttSeries");
        QSignalSpy changed(m_watchedCtrl.get(),
            &controllers::WatchedController::changed);
        m_watchedCtrl->setEpisodesWatched(imdb,
            { { 1, 1 }, { 1, 2 }, { 1, 3 } }, true);
        drain();
        QVERIFY(m_watchedCtrl->isEpisodeWatched(imdb, 1, 1));
        QVERIFY(m_watchedCtrl->isEpisodeWatched(imdb, 1, 2));
        QVERIFY(m_watchedCtrl->isEpisodeWatched(imdb, 1, 3));
        // The store coalesces per event-loop tick, so the controller
        // forwards a single combined `changed()` for the batch.
        QCOMPARE(changed.count(), 1);

        m_watchedCtrl->setEpisodesWatched(imdb,
            { { 1, 1 }, { 1, 2 } }, false);
        drain();
        QVERIFY(!m_watchedCtrl->isEpisodeWatched(imdb, 1, 1));
        QVERIFY(!m_watchedCtrl->isEpisodeWatched(imdb, 1, 2));
        QVERIFY(m_watchedCtrl->isEpisodeWatched(imdb, 1, 3));
    }

    void clearOverrideRevealsHistoryFinished()
    {
        const QString imdb = QStringLiteral("tt1");
        m_history->record(makeFinishedMovieEntry(imdb));
        m_watchedCtrl->setMovieWatched(imdb, false);
        QVERIFY(!m_watchedCtrl->isMovieWatched(imdb));

        domain::PlaybackKey key;
        key.kind = domain::MediaKind::Movie;
        key.imdbId = imdb;
        m_watchedCtrl->clearOverride(key);
        QVERIFY(m_watchedCtrl->isMovieWatched(imdb));
    }

private:
    QString m_emptyToken;
    std::unique_ptr<QTemporaryDir> m_tmp;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_history;
    std::unique_ptr<core::WatchedStore> m_watchedStore;
    KSharedConfigPtr m_config;
    std::unique_ptr<config::AppSettings> m_settings;
    std::unique_ptr<controllers::HistoryController> m_historyCtrl;
    std::unique_ptr<controllers::WatchedController> m_watchedCtrl;
};

QTEST_MAIN(TstWatchedController)
#include "tst_watched_controller.moc"
