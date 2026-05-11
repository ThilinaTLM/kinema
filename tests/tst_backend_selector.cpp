// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Debrid.h"
#include "domain/Download.h"
#include "domain/Media.h"
#include "download/AssetSession.h"
#include "download/BackendSelector.h"
#include "download/DownloadBackend.h"

#include <QTest>

#include <memory>

using namespace kinema;

namespace {

/// Stub backend that records selection lookups via `canHandle` and
/// reports a configurable kind. Never produces a real session.
class StubBackend final : public download::DownloadBackend
{
public:
    StubBackend(domain::DownloadBackendKind kind, bool willHandle)
        : m_kind(kind)
        , m_willHandle(willHandle)
    {
    }

    domain::DownloadBackendKind kind() const noexcept override
    {
        return m_kind;
    }

    bool canHandle(const domain::Stream&) const override
    {
        ++canHandleCalls;
        return m_willHandle;
    }

    QCoro::Task<std::unique_ptr<download::AssetSession>> open(
        const domain::AssetRef&, const domain::Stream&,
        const domain::PlaybackContext&, domain::DownloadMode) override
    {
        co_return nullptr;
    }

    void changeMode(download::AssetSession&, domain::DownloadMode) override
    {
    }

    mutable int canHandleCalls = 0;

private:
    domain::DownloadBackendKind m_kind;
    bool m_willHandle;
};

domain::Stream makeStream()
{
    domain::Stream s;
    s.infoHash = QStringLiteral(
        "aabbccddeeff00112233445566778899aabbccdd");
    return s;
}

} // namespace

class TstBackendSelector : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void rd_active_picksRdEvenWhenAdAlsoHandles()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            domain::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            domain::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            domain::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(domain::DebridProvider::RealDebrid);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), domain::DownloadBackendKind::RealDebridHttp);
        // AD must NOT have been consulted.
        QCOMPARE(ad->canHandleCalls, 0);
    }

    void ad_active_picksAdAndSkipsRd()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            domain::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            domain::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            domain::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(domain::DebridProvider::AllDebrid);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), domain::DownloadBackendKind::AllDebridHttp);
        QCOMPARE(rd->canHandleCalls, 0);
    }

    void none_active_skipsBothDebridBackends()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            domain::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            domain::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            domain::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(domain::DebridProvider::None);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), domain::DownloadBackendKind::Torrent);
        QCOMPARE(rd->canHandleCalls, 0);
        QCOMPARE(ad->canHandleCalls, 0);
    }

    void active_debrid_cannotHandle_fallsThroughToTorrent()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            domain::DownloadBackendKind::RealDebridHttp, false);
        auto* tor = new StubBackend(
            domain::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(domain::DebridProvider::RealDebrid);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), domain::DownloadBackendKind::Torrent);
    }

    void override_reachesNonActiveDebridBackend()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            domain::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            domain::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            domain::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(domain::DebridProvider::AllDebrid);

        // Even though AD is active, an explicit RD override must
        // still resolve so resume of an RD-persisted row works.
        auto* picked = sel.select(makeStream(),
            domain::DownloadBackendKind::RealDebridHttp);
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), domain::DownloadBackendKind::RealDebridHttp);
    }
};

QTEST_MAIN(TstBackendSelector)
#include "tst_backend_selector.moc"
