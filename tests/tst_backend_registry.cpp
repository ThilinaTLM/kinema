// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Debrid.h"
#include "domain/Download.h"
#include "domain/Media.h"
#include "playback/policy/BackendSelectionPolicy.h"
#include "playback/ports/MediaSourcePort.h"
#include "playback/transfer/BackendRegistry.h"

#include <QCoro/QCoroTask>

#include <QString>
#include <QTest>

#include <memory>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::transfer;

namespace {

class FakeSource final : public ports::MediaSourcePort
{
public:
    FakeSource(domain::DownloadBackendKind kind, bool canHandleFlag)
        : m_kind(kind)
        , m_canHandle(canHandleFlag)
    {
    }
    domain::DownloadBackendKind kind() const noexcept override
    {
        return m_kind;
    }
    bool canHandle(const domain::Stream&) const override
    {
        return m_canHandle;
    }
    QCoro::Task<ports::OpenedSession> open(const domain::AssetRef&,
        const domain::Stream&, const domain::PlaybackContext&,
        domain::DownloadMode) override
    {
        co_return ports::OpenedSession {};
    }

    void setCanHandle(bool v) noexcept { m_canHandle = v; }

private:
    domain::DownloadBackendKind m_kind;
    bool m_canHandle;
};

domain::Stream torrentStream()
{
    domain::Stream s;
    s.infoHash = QStringLiteral("aabb");
    return s;
}

} // namespace

class TestBackendRegistry : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void registerAndFindByKind();
    void duplicateKindIsIgnored();
    void noActiveDebridRoutesToTorrent();
    void activeRdRoutesToRealDebrid();
    void activeAdRoutesToAllDebrid();
    void activeDebridUnsupportedReportsError();
    void overrideRoutesEvenWithoutActiveDebrid();
    void overrideMismatchReportsError();
    void noBackendReportsError();
};

void TestBackendRegistry::registerAndFindByKind()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    QVERIFY(reg.find(domain::DownloadBackendKind::Torrent) != nullptr);
    QCOMPARE(reg.find(domain::DownloadBackendKind::Torrent)->kind(),
        domain::DownloadBackendKind::Torrent);
    QCOMPARE(reg.size(), 1);
}

void TestBackendRegistry::duplicateKindIsIgnored()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, false));
    QCOMPARE(reg.size(), 1);
    // First registration wins.
    QVERIFY(reg.find(domain::DownloadBackendKind::Torrent)
                ->canHandle(domain::Stream {}));
}

void TestBackendRegistry::noActiveDebridRoutesToTorrent()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::RealDebridHttp, true));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::AllDebridHttp, true));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    // No active debrid provider — debrid backends are skipped.
    const auto result = reg.select(torrentStream());
    const auto* picked
        = std::get_if<ports::MediaSourcePort*>(&result);
    QVERIFY(picked);
    QCOMPARE((*picked)->kind(), domain::DownloadBackendKind::Torrent);
}

void TestBackendRegistry::activeRdRoutesToRealDebrid()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::RealDebridHttp, true));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    reg.setActiveDebridProvider(domain::DebridProvider::RealDebrid);

    const auto result = reg.select(torrentStream());
    const auto* picked
        = std::get_if<ports::MediaSourcePort*>(&result);
    QVERIFY(picked);
    QCOMPARE((*picked)->kind(),
        domain::DownloadBackendKind::RealDebridHttp);
}

void TestBackendRegistry::activeAdRoutesToAllDebrid()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::AllDebridHttp, true));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    reg.setActiveDebridProvider(domain::DebridProvider::AllDebrid);

    const auto result = reg.select(torrentStream());
    const auto* picked
        = std::get_if<ports::MediaSourcePort*>(&result);
    QVERIFY(picked);
    QCOMPARE((*picked)->kind(),
        domain::DownloadBackendKind::AllDebridHttp);
}

void TestBackendRegistry::activeDebridUnsupportedReportsError()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::RealDebridHttp, /*canHandle*/ false));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    reg.setActiveDebridProvider(domain::DebridProvider::RealDebrid);

    const auto result = reg.select(torrentStream());
    const auto* err
        = std::get_if<policy::BackendSelectionError>(&result);
    QVERIFY(err);
    QCOMPARE(err->kind,
        policy::BackendSelectionError::Kind::ActiveProviderUnsupported);
    QCOMPARE(err->provider, domain::DebridProvider::RealDebrid);
}

void TestBackendRegistry::overrideRoutesEvenWithoutActiveDebrid()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::RealDebridHttp, true));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    // No active debrid, but override forces RD.
    const auto result = reg.select(torrentStream(),
        domain::DownloadBackendKind::RealDebridHttp);
    const auto* picked
        = std::get_if<ports::MediaSourcePort*>(&result);
    QVERIFY(picked);
    QCOMPARE((*picked)->kind(),
        domain::DownloadBackendKind::RealDebridHttp);
}

void TestBackendRegistry::overrideMismatchReportsError()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::RealDebridHttp, /*canHandle*/ false));
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, true));
    const auto result = reg.select(torrentStream(),
        domain::DownloadBackendKind::RealDebridHttp);
    const auto* err
        = std::get_if<policy::BackendSelectionError>(&result);
    QVERIFY(err);
    QCOMPARE(err->kind,
        policy::BackendSelectionError::Kind::OverrideUnsupported);
}

void TestBackendRegistry::noBackendReportsError()
{
    BackendRegistry reg;
    reg.registerSource(std::make_unique<FakeSource>(
        domain::DownloadBackendKind::Torrent, /*canHandle*/ false));
    const auto result = reg.select(torrentStream());
    const auto* err
        = std::get_if<policy::BackendSelectionError>(&result);
    QVERIFY(err);
    QCOMPARE(err->kind, policy::BackendSelectionError::Kind::NoBackend);
}

QTEST_MAIN(TestBackendRegistry)
#include "tst_backend_registry.moc"
