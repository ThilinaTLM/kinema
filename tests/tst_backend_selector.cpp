// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Debrid.h"
#include "api/Download.h"
#include "api/Media.h"
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
    StubBackend(api::DownloadBackendKind kind, bool willHandle)
        : m_kind(kind)
        , m_willHandle(willHandle)
    {
    }

    api::DownloadBackendKind kind() const noexcept override
    {
        return m_kind;
    }

    bool canHandle(const api::Stream&) const override
    {
        ++canHandleCalls;
        return m_willHandle;
    }

    QCoro::Task<std::unique_ptr<download::AssetSession>> open(
        const api::AssetRef&, const api::Stream&,
        const api::PlaybackContext&, api::DownloadMode) override
    {
        co_return nullptr;
    }

    void changeMode(download::AssetSession&, api::DownloadMode) override
    {
    }

    mutable int canHandleCalls = 0;

private:
    api::DownloadBackendKind m_kind;
    bool m_willHandle;
};

api::Stream makeStream()
{
    api::Stream s;
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
            api::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            api::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            api::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(api::DebridProvider::RealDebrid);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), api::DownloadBackendKind::RealDebridHttp);
        // AD must NOT have been consulted.
        QCOMPARE(ad->canHandleCalls, 0);
    }

    void ad_active_picksAdAndSkipsRd()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            api::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            api::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            api::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(api::DebridProvider::AllDebrid);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), api::DownloadBackendKind::AllDebridHttp);
        QCOMPARE(rd->canHandleCalls, 0);
    }

    void none_active_skipsBothDebridBackends()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            api::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            api::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            api::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(api::DebridProvider::None);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), api::DownloadBackendKind::Torrent);
        QCOMPARE(rd->canHandleCalls, 0);
        QCOMPARE(ad->canHandleCalls, 0);
    }

    void active_debrid_cannotHandle_fallsThroughToTorrent()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            api::DownloadBackendKind::RealDebridHttp, false);
        auto* tor = new StubBackend(
            api::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(api::DebridProvider::RealDebrid);

        auto* picked = sel.select(makeStream());
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), api::DownloadBackendKind::Torrent);
    }

    void override_reachesNonActiveDebridBackend()
    {
        download::BackendSelector sel;
        auto* rd = new StubBackend(
            api::DownloadBackendKind::RealDebridHttp, true);
        auto* ad = new StubBackend(
            api::DownloadBackendKind::AllDebridHttp, true);
        auto* tor = new StubBackend(
            api::DownloadBackendKind::Torrent, true);
        sel.registerBackend(std::unique_ptr<StubBackend>(rd));
        sel.registerBackend(std::unique_ptr<StubBackend>(ad));
        sel.registerBackend(std::unique_ptr<StubBackend>(tor));
        sel.setActiveDebridProvider(api::DebridProvider::AllDebrid);

        // Even though AD is active, an explicit RD override must
        // still resolve so resume of an RD-persisted row works.
        auto* picked = sel.select(makeStream(),
            api::DownloadBackendKind::RealDebridHttp);
        QVERIFY(picked != nullptr);
        QCOMPARE(picked->kind(), api::DownloadBackendKind::RealDebridHttp);
    }
};

QTEST_MAIN(TstBackendSelector)
#include "tst_backend_selector.moc"
