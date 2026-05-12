// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/PlaybackContext.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "core/persistence/Database.h"
#include "core/persistence/HistoryStore.h"
#include "ui/qml-bridge/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/LibraryRailModel.h"

#include <KSharedConfig>

#include <QDateTime>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using kinema::domain::HistoryEntry;
using kinema::domain::HistoryStreamRef;
using kinema::domain::MediaKind;
using kinema::domain::PlaybackKey;
using kinema::config::AppSettings;
using kinema::controllers::HistoryController;
using kinema::core::Database;
using kinema::core::HistoryStore;
using kinema::ui::qml::ContinueWatchingViewModel;
using kinema::ui::qml::LibraryRailModel;

namespace {

HistoryEntry makeMovieEntry(const QString& imdb, double pos,
    double dur, const QString& release = QStringLiteral("Release.Name"))
{
    HistoryEntry e;
    e.key.kind = MediaKind::Movie;
    e.key.imdbId = imdb;
    e.title = QStringLiteral("Movie ") + imdb;
    e.poster = QUrl(QStringLiteral("https://example.com/") + imdb
        + QStringLiteral(".jpg"));
    e.backdrop = QUrl(QStringLiteral("https://example.com/") + imdb
        + QStringLiteral("-backdrop.jpg"));
    e.positionSec = pos;
    e.durationSec = dur;
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();
    e.lastStream.infoHash = QStringLiteral("hash-") + imdb;
    e.lastStream.releaseName = release;
    e.lastStream.resolution = QStringLiteral("1080p");
    return e;
}

void drain()
{
    for (int i = 0; i < 4; ++i) {
        QCoreApplication::processEvents();
    }
}

} // namespace

class TstContinueWatchingViewModel : public QObject
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
        m_db = std::make_unique<Database>(
            m_tmp->filePath(QStringLiteral("kinema.db")), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<HistoryStore>(*m_db);
        m_config = KSharedConfig::openConfig(
            m_tmp->filePath(QStringLiteral("kinemarc")));
        m_settings = std::make_unique<AppSettings>(m_config, nullptr);
        m_history = std::make_unique<HistoryController>(*m_store,
            /*indexers=*/nullptr, m_emptyToken);
    }

    void cleanup()
    {
        m_history.reset();
        m_settings.reset();
        m_config.reset();
        m_store.reset();
        m_db.reset();
        m_tmp.reset();
    }

    // ---- Tests --------------------------------------------------------

    void emptyHistoryStartsEmpty()
    {
        ContinueWatchingViewModel vm(m_history.get());
        QVERIFY(vm.empty());
        QCOMPARE(vm.entries().size(), 0);
        QVERIFY(vm.model() != nullptr);
        QCOMPARE(vm.model()->rowCount(), 0);
    }

    void recordingAnEntryFlipsEmptyAndFillsModel()
    {
        ContinueWatchingViewModel vm(m_history.get());
        QSignalSpy emptySpy(&vm, &ContinueWatchingViewModel::emptyChanged);

        m_store->record(makeMovieEntry(
            QStringLiteral("tt1000001"), 600, 6000));
        // HistoryStore::changed() is coalesced via a queued signal —
        // the VM's slot fires once we drain.
        drain();

        QVERIFY(!vm.empty());
        QCOMPARE(vm.entries().size(), 1);
        QCOMPARE(vm.model()->rowCount(), 1);
        QCOMPARE(emptySpy.count(), 1);

        // Display fields surface through the LibraryRailModel.
        const auto* row = vm.model()->at(0);
        QVERIFY(row != nullptr);
        QCOMPARE(row->title, QStringLiteral("Movie tt1000001"));
        QCOMPARE(row->primaryLine, QStringLiteral("Movie tt1000001"));
        // Movies have no secondary line (matches Ready to Watch's
        // movie handling): the meta block collapses to two lines.
        QVERIFY(row->secondaryLine.isEmpty());

        // Progress overlay reflects the entry's position / duration.
        const auto idx = vm.model()->index(0);
        const auto progress = vm.model()->data(idx,
            LibraryRailModel::ProgressRole).toDouble();
        QCOMPARE(progress, 0.1);

        // Tertiary line: "Resume from 10%" for an in-progress entry.
        const auto tertiary = vm.model()->data(idx,
            LibraryRailModel::TertiaryLineRole).toString();
        QVERIFY(tertiary.contains(QStringLiteral("10")));

        // Backdrop captured at play time round-trips through the
        // store into the rail row so `EpisodeRailCard` can render a
        // proper 16:9 frame instead of letterboxing the poster.
        const auto backdrop = vm.model()->data(idx,
            LibraryRailModel::BackdropUrlRole).toString();
        QVERIFY(backdrop.contains(QStringLiteral("-backdrop.jpg")));
    }

    void resumeForwardsTheRightEntry()
    {
        ContinueWatchingViewModel vm(m_history.get());
        m_store->record(makeMovieEntry(QStringLiteral("tt2"), 60, 6000));
        m_store->record(makeMovieEntry(QStringLiteral("tt1"), 120, 6000));
        drain();

        QSignalSpy resumeSpy(&vm, &ContinueWatchingViewModel::resumeRequested);
        // Most-recently-watched first; both records share the same
        // wall-clock millisecond on a fast machine, but tt1 was
        // recorded second so it sits at row 0.
        vm.resume(0);
        QCOMPARE(resumeSpy.count(), 1);
        const auto firstEntry = vm.entries().at(0);
        const auto signalEntry = qvariant_cast<HistoryEntry>(
            resumeSpy.first().at(0));
        QCOMPARE(signalEntry.key.imdbId, firstEntry.key.imdbId);
    }

    void openDetailForwardsEntry()
    {
        ContinueWatchingViewModel vm(m_history.get());
        m_store->record(makeMovieEntry(QStringLiteral("tt5"), 30, 6000));
        drain();

        QSignalSpy detailSpy(&vm, &ContinueWatchingViewModel::detailRequested);
        vm.openDetail(0);
        QCOMPARE(detailSpy.count(), 1);
        QCOMPARE(qvariant_cast<HistoryEntry>(detailSpy.first().at(0))
                     .key.imdbId,
            QStringLiteral("tt5"));
    }

    void openStreamsForwardsEntryAndOutOfRangeIsNoOp()
    {
        ContinueWatchingViewModel vm(m_history.get());
        m_store->record(makeMovieEntry(QStringLiteral("tt6"), 45, 6000));
        drain();

        QSignalSpy streamsSpy(&vm,
            &ContinueWatchingViewModel::streamsRequested);
        vm.openStreams(99);
        QCOMPARE(streamsSpy.count(), 0);
        vm.openStreams(-1);
        QCOMPARE(streamsSpy.count(), 0);

        vm.openStreams(0);
        QCOMPARE(streamsSpy.count(), 1);
        QCOMPARE(qvariant_cast<HistoryEntry>(streamsSpy.first().at(0))
                     .key.imdbId,
            QStringLiteral("tt6"));
    }

    void removeForwardsAndOutOfRangeIsNoOp()
    {
        ContinueWatchingViewModel vm(m_history.get());
        m_store->record(makeMovieEntry(QStringLiteral("tt9"), 30, 6000));
        drain();

        QSignalSpy removeSpy(&vm, &ContinueWatchingViewModel::removeRequested);
        vm.remove(99);
        QCOMPARE(removeSpy.count(), 0);
        vm.remove(-1);
        QCOMPARE(removeSpy.count(), 0);

        vm.remove(0);
        QCOMPARE(removeSpy.count(), 1);
    }

    void controllerRemoveEntryDropsTheRow()
    {
        ContinueWatchingViewModel vm(m_history.get());
        const auto e = makeMovieEntry(QStringLiteral("tt7"), 30, 6000);
        m_store->record(e);
        drain();
        QCOMPARE(vm.entries().size(), 1);

        QSignalSpy emptySpy(&vm, &ContinueWatchingViewModel::emptyChanged);

        // Going through the controller exercises the removeEntry()
        // slot we added in phase 03; it must call into the store and
        // the rail must collapse on the next changed() tick.
        m_history->removeEntry(e);
        drain();

        QVERIFY(vm.empty());
        QCOMPARE(vm.entries().size(), 0);
        QCOMPARE(emptySpy.count(), 1);
    }

    void emptyChangedDoesNotFireForUnchangedState()
    {
        ContinueWatchingViewModel vm(m_history.get());
        QSignalSpy emptySpy(&vm, &ContinueWatchingViewModel::emptyChanged);

        // Manual refresh on an already-empty controller should NOT
        // emit emptyChanged: the VM caches the last reported state.
        vm.refresh();
        drain();
        QCOMPARE(emptySpy.count(), 0);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    std::unique_ptr<Database> m_db;
    std::unique_ptr<HistoryStore> m_store;
    KSharedConfigPtr m_config;
    std::unique_ptr<AppSettings> m_settings;
    std::unique_ptr<HistoryController> m_history;
    QString m_emptyToken;
};

QTEST_MAIN(TstContinueWatchingViewModel)
#include "tst_continue_watching_view_model.moc"
