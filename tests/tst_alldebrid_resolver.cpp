// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/AllDebridClient.h"
#include "domain/Download.h"
#include "core/io/HttpError.h"
#include "download/AllDebridResolver.h"
#include "TestDoubles.h"

#include <QCoroSignal>
#include <QCoroTask>
#include <QSignalSpy>
#include <QTest>
#include <QTimer>

using namespace kinema;

namespace {

/// In-memory `AllDebridClient` stub that lets each test script the
/// upload / status / files / unlock / delete responses (or errors)
/// in order. Each method records the calls so tests can assert
/// dispatch order and arguments.
class StubAllDebridClient : public api::AllDebridClient
{
public:
    StubAllDebridClient()
        : api::AllDebridClient(nullptr)
    {
    }

    // Scripted responses.
    QList<domain::AdAddMagnetResult> uploadReplies;
    QList<domain::AdMagnetStatus> statusReplies;
    QList<QList<domain::AdMagnetFile>> filesReplies;
    QList<domain::AdUnlockedLink> unlockReplies;
    std::optional<core::HttpError> uploadError;
    std::optional<core::HttpError> statusError;
    std::optional<core::HttpError> filesError;
    std::optional<core::HttpError> unlockError;

    // Recorded calls.
    int uploadCalls = 0;
    int statusCalls = 0;
    int filesCalls = 0;
    int unlockCalls = 0;
    int deleteCalls = 0;
    QString lastUploadMagnet;
    qint64 lastDeletedId = 0;
    QUrl lastUnlockedLink;

    QCoro::Task<domain::AllDebridUser> user() override
    {
        co_return domain::AllDebridUser {};
    }

    QCoro::Task<domain::AdAddMagnetResult> uploadMagnet(QString magnet) override
    {
        ++uploadCalls;
        lastUploadMagnet = magnet;
        if (uploadError) {
            throw *uploadError;
        }
        if (uploadReplies.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json, 0,
                QStringLiteral("StubAllDebridClient: no upload reply"));
        }
        co_return uploadReplies.takeFirst();
    }

    QCoro::Task<domain::AdMagnetStatus> magnetStatus(qint64) override
    {
        ++statusCalls;
        if (statusError) {
            throw *statusError;
        }
        if (statusReplies.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json, 0,
                QStringLiteral("StubAllDebridClient: no status reply"));
        }
        co_return statusReplies.takeFirst();
    }

    QCoro::Task<QList<domain::AdMagnetFile>> magnetFiles(qint64) override
    {
        ++filesCalls;
        if (filesError) {
            throw *filesError;
        }
        if (filesReplies.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json, 0,
                QStringLiteral("StubAllDebridClient: no files reply"));
        }
        co_return filesReplies.takeFirst();
    }

    QCoro::Task<domain::AdUnlockedLink> unlockLink(QUrl link) override
    {
        ++unlockCalls;
        lastUnlockedLink = link;
        if (unlockError) {
            throw *unlockError;
        }
        if (unlockReplies.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json, 0,
                QStringLiteral("StubAllDebridClient: no unlock reply"));
        }
        co_return unlockReplies.takeFirst();
    }

    QCoro::Task<void> deleteMagnet(qint64 id) override
    {
        ++deleteCalls;
        lastDeletedId = id;
        co_return;
    }
};

domain::AssetRef makeRef(const QString& hint = QString())
{
    domain::AssetRef ref;
    ref.key.kind = domain::MediaKind::Movie;
    ref.key.imdbId = QStringLiteral("tt1");
    ref.infoHash = QStringLiteral(
        "aabbccddeeff00112233445566778899aabbccdd");
    ref.releaseName = QStringLiteral("Movie.Release");
    ref.fileNameHint = hint;
    return ref;
}

domain::AdAddMagnetResult makeUploadOk(qint64 id = 999)
{
    domain::AdAddMagnetResult r;
    r.id = id;
    r.hash = QStringLiteral("aabbccddeeff00112233445566778899aabbccdd");
    r.name = QStringLiteral("Movie.Release");
    r.sizeBytes = 1'000'000;
    r.ready = true;
    return r;
}

domain::AdMagnetStatus makeStatus(int code)
{
    domain::AdMagnetStatus s;
    s.id = 999;
    s.statusCode = code;
    s.status = code == 4 ? QStringLiteral("Ready")
                         : QStringLiteral("Other");
    return s;
}

domain::AdMagnetFile makeFile(const QString& path, qint64 bytes,
    const QString& url)
{
    domain::AdMagnetFile f;
    f.path = path;
    f.bytes = bytes;
    f.hosterLink = QUrl(url);
    return f;
}

} // namespace

class TstAllDebridResolver : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void resolve_pickByFileNameHint()
    {
        StubAllDebridClient stub;
        stub.uploadReplies = { makeUploadOk() };
        stub.statusReplies = { makeStatus(4) };
        stub.filesReplies = {
            QList<domain::AdMagnetFile> {
                makeFile(QStringLiteral("Other.Show.1080p.mkv"), 1'500'000'000,
                    QStringLiteral("https://alldebrid.com/f/other")),
                makeFile(QStringLiteral("Movie.Release.1080p.mkv"), 1'600'000'000,
                    QStringLiteral("https://alldebrid.com/f/wanted")),
            }
        };
        domain::AdUnlockedLink unlock;
        unlock.download = QUrl(QStringLiteral(
            "https://p1.alldeb.ovh/dl/wanted.mkv"));
        unlock.fileSize = 1'600'000'000;
        unlock.filename = QStringLiteral("Movie.Release.1080p.mkv");
        stub.unlockReplies = { unlock };

        download::AllDebridResolver r(stub);
        const auto out = QCoro::waitFor(r.resolve(
            makeRef(QStringLiteral("Movie.Release.1080p.mkv"))));

        QCOMPARE(stub.uploadCalls, 1);
        QCOMPARE(stub.filesCalls, 1);
        QCOMPARE(stub.unlockCalls, 1);
        QCOMPARE(stub.lastUnlockedLink,
            QUrl(QStringLiteral("https://alldebrid.com/f/wanted")));
        QCOMPARE(out.downloadUrl,
            QUrl(QStringLiteral("https://p1.alldeb.ovh/dl/wanted.mkv")));
        QCOMPARE(out.fileSize, 1'600'000'000LL);
        QCOMPARE(out.providerTorrentId, QStringLiteral("999"));
    }

    void resolve_pollsUntilReady()
    {
        StubAllDebridClient stub;
        stub.uploadReplies = { makeUploadOk() };
        // Two polls before Ready.
        stub.statusReplies = {
            makeStatus(1), makeStatus(1), makeStatus(4),
        };
        stub.filesReplies = {
            QList<domain::AdMagnetFile> {
                makeFile(QStringLiteral("Movie.mkv"), 1'500'000'000,
                    QStringLiteral("https://alldebrid.com/f/m"))
            }
        };
        domain::AdUnlockedLink unlock;
        unlock.download = QUrl(QStringLiteral("https://p1/dl/m.mkv"));
        unlock.fileSize = 1'500'000'000;
        stub.unlockReplies = { unlock };

        download::AllDebridResolver r(stub);
        const auto out = QCoro::waitFor(r.resolve(makeRef()));

        QCOMPARE(stub.statusCalls, 3);
        QVERIFY(!out.downloadUrl.isEmpty());
    }

    void resolve_terminalStatusThrows()
    {
        StubAllDebridClient stub;
        stub.uploadReplies = { makeUploadOk() };
        stub.statusReplies = { makeStatus(8) }; // 8 = File too big.

        download::AllDebridResolver r(stub);
        try {
            (void)QCoro::waitFor(r.resolve(makeRef()));
            QFAIL("expected HttpError");
        } catch (const core::HttpError& e) {
            QVERIFY(e.message().contains(QStringLiteral("8")));
        }
    }

    void resolve_emptyFilesThrows()
    {
        StubAllDebridClient stub;
        stub.uploadReplies = { makeUploadOk() };
        stub.statusReplies = { makeStatus(4) };
        stub.filesReplies = { QList<domain::AdMagnetFile> {} };

        download::AllDebridResolver r(stub);
        try {
            (void)QCoro::waitFor(r.resolve(makeRef()));
            QFAIL("expected HttpError");
        } catch (const core::HttpError& e) {
            QVERIFY(e.message().contains(QStringLiteral("file"),
                Qt::CaseInsensitive));
        }
    }

    void cleanup_callsDeleteWithDecodedId()
    {
        StubAllDebridClient stub;
        download::AllDebridResolver r(stub);
        QCoro::waitFor(r.cleanup(QStringLiteral("123")));
        QCOMPARE(stub.deleteCalls, 1);
        QCOMPARE(stub.lastDeletedId, 123LL);
    }

    void cleanup_emptyIsNoop()
    {
        StubAllDebridClient stub;
        download::AllDebridResolver r(stub);
        QCoro::waitFor(r.cleanup(QString {}));
        QCOMPARE(stub.deleteCalls, 0);
    }

    void resolve_seriesEpisodeFavoursMatchingPath()
    {
        StubAllDebridClient stub;
        stub.uploadReplies = { makeUploadOk() };
        stub.statusReplies = { makeStatus(4) };
        stub.filesReplies = {
            QList<domain::AdMagnetFile> {
                makeFile(QStringLiteral("Show.S01E01.1080p.mkv"), 1'400'000'000,
                    QStringLiteral("https://alldebrid.com/f/ep1")),
                makeFile(QStringLiteral("Show.S01E02.1080p.mkv"), 1'500'000'000,
                    QStringLiteral("https://alldebrid.com/f/ep2")),
                makeFile(QStringLiteral("Show.S01E03.1080p.mkv"), 1'600'000'000,
                    QStringLiteral("https://alldebrid.com/f/ep3")),
            }
        };
        domain::AdUnlockedLink unlock;
        unlock.download = QUrl(QStringLiteral("https://p1/dl/ep2.mkv"));
        unlock.fileSize = 1'500'000'000;
        stub.unlockReplies = { unlock };

        domain::AssetRef ref;
        ref.key.kind = domain::MediaKind::Series;
        ref.key.imdbId = QStringLiteral("tt2");
        ref.key.season = 1;
        ref.key.episode = 2;
        ref.infoHash = QStringLiteral(
            "1122334455667788990011223344556677889900");
        ref.releaseName = QStringLiteral("Show.S01");

        download::AllDebridResolver r(stub);
        const auto out = QCoro::waitFor(r.resolve(ref));
        QCOMPARE(stub.lastUnlockedLink,
            QUrl(QStringLiteral("https://alldebrid.com/f/ep2")));
        QCOMPARE(out.downloadUrl,
            QUrl(QStringLiteral("https://p1/dl/ep2.mkv")));
    }
};

QTEST_MAIN(TstAllDebridResolver)
#include "tst_alldebrid_resolver.moc"
