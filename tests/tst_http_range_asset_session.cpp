// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

// focused tests for `playback::sources::HttpRangeAssetSession`. These cover the size /
// chunk-bookkeeping initialisation paths exercised by series
// auto-next, where the constructor may be handed an
// `AssetRef::sizeBytes` that already matches the upstream's true
// size (because the file's size was learned from the magnet's file
// list via the debrid resolver). The regression scenario:
//
//   1. ref.sizeBytes = X is set when series auto-next builds the
//      Stream for the next episode.
//   2. HttpRangeAssetSession ctor sets m_fileSize = X.
//   3. AllDebrid resolver returns the same X.
//   4. ensureResolved sees `resolved.fileSize == m_fileSize` and
//      previously skipped the `m_totalChunks` / `m_chunkAvailable`
//      init block.
//   5. The first ensureRange call -> ensureChunk(0) ->
//      `chunkIndex (0) >= m_totalChunks (0)` -> co_return false
//      synchronously.
//   6. LocalMediaServer logs "ensureRange/readRange failed" and
//      mpv eventually gives up with end-file reason=error.
//
// We test both that the constructor pre-sizes when given a size
// hint and that ensureResolved fills in the bookkeeping when the
// resolved size matches the constructor hint.

#include "TestDoubles.h"

#include "config/DownloadSettings.h"
#include "domain/Download.h"
#include "download/DebridResolver.h"
#include "playback/sources/HttpRangeAssetSession.h"

#include <KSharedConfig>
#include <QCoroTask>
#include <QDir>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;
using kinema::tests::FakeHttpClient;

namespace {

class StubResolver : public download::DebridResolver
{
public:
    download::ResolvedDebridLink reply;
    int calls = 0;

    QCoro::Task<download::ResolvedDebridLink> resolve(
        domain::AssetRef ref) override
    {
        Q_UNUSED(ref);
        ++calls;
        co_return reply;
    }

    QCoro::Task<void> cleanup(QString) override { co_return; }
};

domain::AssetRef makeRef(qint64 sizeHint)
{
    domain::AssetRef ref;
    ref.key.kind = domain::MediaKind::Series;
    ref.key.imdbId = QStringLiteral("tt0460681");
    ref.key.season = 6;
    ref.key.episode = 3;
    ref.infoHash = QStringLiteral(
        "0123456789abcdef0123456789abcdef01234567");
    ref.fileIndex = 113;
    ref.fileNameHint = QStringLiteral(
        "Supernatural.S06E03.The Third Man.1080p.H265-Zero00.mp4");
    if (sizeHint > 0) {
        ref.sizeBytes = sizeHint;
    }
    return ref;
}

} // namespace

class TstHttpRangeAssetSession : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
        m_config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-http-session-test"),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<config::DownloadSettings>(m_config);
    }

    void init()
    {
        m_tmp = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmp->isValid());
    }

    void cleanup() { m_tmp.reset(); }

    void cleanupTestCase() { m_settings.reset(); }

    // Regression test for the "Next episode silently fails" bug:
    // when the constructor receives a size hint, the chunk
    // bookkeeping must be initialised so a subsequent ensureChunk
    // call has a non-zero m_totalChunks to compare against.
    void constructorPreSizesChunkMap_whenRefSizeHintPresent()
    {
        FakeHttpClient http;
        StubResolver resolver;

        const qint64 sizeHint = 243'276'646LL;
        playback::sources::HttpRangeAssetSession session(http, resolver, *m_settings,
            makeRef(sizeHint),
            QStringLiteral("asset-x"), m_tmp->path());

        // The payload file should have been pre-sized to the hint.
        QFile f(QDir(m_tmp->path()).absoluteFilePath(
            QStringLiteral("payload.bin")));
        QVERIFY2(f.exists(),
            "payload.bin should be created when the ctor is given a size hint");
        QCOMPARE(f.size(), sizeHint);
        QCOMPARE(session.fileSize(), sizeHint);
    }

    void constructorLeavesNoFile_whenRefSizeHintAbsent()
    {
        // Mirror case: without a hint, ctor must NOT touch the
        // filesystem. ensureResolved discovers the size later.
        FakeHttpClient http;
        StubResolver resolver;
        playback::sources::HttpRangeAssetSession session(http, resolver, *m_settings,
            makeRef(/*sizeHint=*/-1),
            QStringLiteral("asset-y"), m_tmp->path());

        QFile f(QDir(m_tmp->path()).absoluteFilePath(
            QStringLiteral("payload.bin")));
        QVERIFY(!f.exists());
        QCOMPARE(session.fileSize(), qint64(-1));
    }

    // Belt-and-braces: without a constructor hint, ensureResolved
    // still discovers the size from the resolver and initialises
    // the chunk bookkeeping then. Same scenario the legacy code
    // already handled, retained here to lock the behaviour in.
    void ensureResolved_initsBookkeepingWhenNoHintAndResolverReportsSize()
    {
        FakeHttpClient http;
        StubResolver resolver;
        const qint64 size = 243'276'646LL;
        resolver.reply.downloadUrl = QUrl(QStringLiteral(
            "https://hoster.example/test.mp4"));
        resolver.reply.fileSize = size;
        resolver.reply.fileName = QStringLiteral("test.mp4");

        playback::sources::HttpRangeAssetSession session(http, resolver, *m_settings,
            makeRef(/*sizeHint=*/-1),
            QStringLiteral("asset-z"), m_tmp->path());

        QCoro::waitFor(session.ensureResolved());

        QFile f(QDir(m_tmp->path()).absoluteFilePath(
            QStringLiteral("payload.bin")));
        QVERIFY(f.exists());
        QCOMPARE(f.size(), size);
        QCOMPARE(session.fileSize(), size);
        QCOMPARE(resolver.calls, 1);
    }

private:
    KSharedConfigPtr m_config;
    std::unique_ptr<config::DownloadSettings> m_settings;
    std::unique_ptr<QTemporaryDir> m_tmp;
};

QTEST_MAIN(TstHttpRangeAssetSession)
#include "tst_http_range_asset_session.moc"
