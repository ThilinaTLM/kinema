// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/PlaybackContext.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "core/Database.h"
#include "core/HistoryStore.h"
#include "ui/qml-bridge/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KSharedConfig>

#include <QDateTime>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using kinema::api::HistoryEntry;
using kinema::api::HistoryStreamRef;
using kinema::api::MediaKind;
using kinema::api::PlaybackKey;
using kinema::config::AppSettings;
using kinema::controllers::HistoryController;
using kinema::core::Database;
using kinema::core::HistoryStore;
using kinema::ui::qml::ContinueWatchingViewModel;
using kinema::ui::qml::DiscoverSectionModel;

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
            /*torrentio=*/nullptr, *m_settings, m_emptyToken);
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

        // Display fields surface through the section model with named
        // roles.
        const auto* item = vm.model()->itemAt(0);
        QVERIFY(item != nullptr);
        QCOMPARE(item->title, QStringLiteral("Movie tt1000001"));

        // Progress overlay reflects the entry's position / duration.
        const auto progressIdx = vm.model()->index(0);
        const auto progress = vm.model()->data(progressIdx,
            DiscoverSectionModel::ProgressRole).toDouble();
        QCOMPARE(progress, 0.1);

        // Last-release subtitle line: "1080p — Release.Name".
        const auto release = vm.model()->data(progressIdx,
            DiscoverSectionModel::LastReleaseRole).toString();
        QVERIFY(release.contains(QStringLiteral("1080p")));
        QVERIFY(release.contains(QStringLiteral("Release.Name")));
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
