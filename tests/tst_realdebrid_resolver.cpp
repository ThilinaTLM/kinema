// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/realdebrid/RealDebridClient.h"
#include "core/io/HttpError.h"
#include "domain/Download.h"
#include "playback/sources/RealDebridResolver.h"

#include <QCoroTask>
#include <QTest>

using namespace kinema;

namespace {

class StubRealDebridClient : public api::RealDebridClient
{
public:
    StubRealDebridClient() : api::RealDebridClient(nullptr) { }

    QList<domain::RdAddMagnetResult> addReplies;
    QList<domain::RdTorrentInfo> infoReplies;
    QList<domain::RdUnrestrictedLink> unrestrictReplies;

    int addCalls = 0;
    int infoCalls = 0;
    int selectCalls = 0;
    int unrestrictCalls = 0;
    QList<int> lastSelectedIds;
    QUrl lastUnrestrictedLink;

    QCoro::Task<domain::RealDebridUser> user() override { co_return domain::RealDebridUser{}; }

    QCoro::Task<domain::RdAddMagnetResult> addMagnet(QString) override
    {
        ++addCalls;
        if (addReplies.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json,
                                  0,
                                  QStringLiteral("StubRealDebridClient: no add reply"));
        }
        co_return addReplies.takeFirst();
    }

    QCoro::Task<domain::RdTorrentInfo> torrentInfo(QString) override
    {
        ++infoCalls;
        if (infoReplies.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json,
                                  0,
                                  QStringLiteral("StubRealDebridClient: no info reply"));
        }
        co_return infoReplies.takeFirst();
    }

    QCoro::Task<void> selectFiles(QString, QList<int> fileIds) override
    {
        ++selectCalls;
        lastSelectedIds = fileIds;
        co_return;
    }

    QCoro::Task<domain::RdUnrestrictedLink> unrestrictLink(QUrl link) override
    {
        ++unrestrictCalls;
        lastUnrestrictedLink = link;
        if (unrestrictReplies.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json,
                                  0,
                                  QStringLiteral("StubRealDebridClient: no unrestrict reply"));
        }
        co_return unrestrictReplies.takeFirst();
    }

    QCoro::Task<void> deleteTorrent(QString) override { co_return; }
};

domain::RdAddMagnetResult addOk()
{
    domain::RdAddMagnetResult r;
    r.id = QStringLiteral("rd-1");
    return r;
}

domain::RdTorrentFile rdFile(int id, const QString& path, qint64 bytes)
{
    domain::RdTorrentFile f;
    f.id = id;
    f.path = path;
    f.bytes = bytes;
    return f;
}

domain::RdTorrentInfo infoWithFiles(QList<domain::RdTorrentFile> files)
{
    domain::RdTorrentInfo info;
    info.id = QStringLiteral("rd-1");
    info.status = QStringLiteral("magnet_conversion");
    info.files = std::move(files);
    return info;
}

domain::RdTorrentInfo readyWithLink(QList<domain::RdTorrentFile> files, const QUrl& link)
{
    auto info = infoWithFiles(std::move(files));
    info.status = QStringLiteral("downloaded");
    info.links = {link};
    return info;
}

domain::RdUnrestrictedLink unrestricted(const QString& fileName)
{
    domain::RdUnrestrictedLink link;
    link.filename = fileName;
    link.fileSize = 1'500'000'000LL;
    link.download = QUrl(QStringLiteral("https://rd.example/dl/") + fileName);
    return link;
}

domain::AssetRef
seriesRef(int episode, int positionalIndex, const QString& fileNameHint = QString())
{
    domain::AssetRef ref;
    ref.key.kind = domain::MediaKind::Series;
    ref.key.imdbId = QStringLiteral("tt2");
    ref.key.season = 1;
    ref.key.episode = episode;
    ref.infoHash = QStringLiteral("1122334455667788990011223344556677889900");
    ref.releaseName = QStringLiteral("Show.S01");
    ref.fileIndex = positionalIndex;
    ref.fileNameHint = fileNameHint;
    return ref;
}

domain::AssetRef movieRef(int fileIndex)
{
    domain::AssetRef ref;
    ref.key.kind = domain::MediaKind::Movie;
    ref.key.imdbId = QStringLiteral("tt1");
    ref.infoHash = QStringLiteral("aabbccddeeff00112233445566778899aabbccdd");
    ref.releaseName = QStringLiteral("Movie.Release");
    ref.fileIndex = fileIndex;
    return ref;
}

} // namespace

class TstRealDebridResolver : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void resolve_seriesAutoNextPrefersEpisodeMatchOverDriftedFileId()
    {
        // Auto-next's fileIndex comes from the already-resolved RD catalog
        // position. RD ids can be non-positional; id=(fileIndex+1) exists
        // here but points to the previous episode. The resolver must follow
        // the requested episode / filename instead.
        const QList<domain::RdTorrentFile> files{
            rdFile(2, QStringLiteral("Show.S01E06.1080p.mkv"), 1'400'000'000LL),
            rdFile(10, QStringLiteral("Show.S01E07.1080p.mkv"), 1'500'000'000LL),
        };

        StubRealDebridClient stub;
        stub.addReplies = {addOk()};
        stub.infoReplies = {
            infoWithFiles(files),
            readyWithLink(files, QUrl(QStringLiteral("https://rd/hoster/ep7"))),
        };
        stub.unrestrictReplies = {
            unrestricted(QStringLiteral("Show.S01E07.1080p.mkv")),
        };

        playback::sources::RealDebridResolver resolver(stub);
        const auto out = QCoro::waitFor(
            resolver.resolve(seriesRef(7, 1, QStringLiteral("Show.S01E07.1080p.mkv"))));

        QCOMPARE(stub.selectCalls, 1);
        QCOMPARE(stub.lastSelectedIds, QList<int>{10});
        QCOMPARE(stub.lastUnrestrictedLink, QUrl(QStringLiteral("https://rd/hoster/ep7")));
        QCOMPARE(out.fileName, QStringLiteral("Show.S01E07.1080p.mkv"));
    }

    void resolve_withoutStrongHintStillUsesFileIndexIdFallback()
    {
        const QList<domain::RdTorrentFile> files{
            rdFile(1, QStringLiteral("Movie.PartA.mkv"), 1'500'000'000LL),
            rdFile(2, QStringLiteral("Movie.PartB.mkv"), 1'400'000'000LL),
        };

        StubRealDebridClient stub;
        stub.addReplies = {addOk()};
        stub.infoReplies = {
            infoWithFiles(files),
            readyWithLink(files, QUrl(QStringLiteral("https://rd/hoster/partB"))),
        };
        stub.unrestrictReplies = {
            unrestricted(QStringLiteral("Movie.PartB.mkv")),
        };

        playback::sources::RealDebridResolver resolver(stub);
        (void)QCoro::waitFor(resolver.resolve(movieRef(/*fileIndex=*/1)));

        QCOMPARE(stub.selectCalls, 1);
        QCOMPARE(stub.lastSelectedIds, QList<int>{2});
    }
};

QTEST_MAIN(TstRealDebridResolver)
#include "tst_realdebrid_resolver.moc"
