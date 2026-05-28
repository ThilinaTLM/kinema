// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/BackendSelectionPolicy.h"

#include <QTest>

using namespace kinema;
using namespace kinema::playback::policy;

namespace {

domain::Stream makeStream()
{
    domain::Stream s;
    s.infoHash = QStringLiteral(
        "aabbccddeeff00112233445566778899aabbccdd");
    return s;
}

BackendSelectionInputs base()
{
    BackendSelectionInputs in;
    in.stream = makeStream();
    in.backends = {
        { domain::DownloadBackendKind::RealDebridHttp, true },
        { domain::DownloadBackendKind::AllDebridHttp, true },
        { domain::DownloadBackendKind::Torrent, true },
    };
    return in;
}

bool isKind(const BackendSelectionResult& r,
    domain::DownloadBackendKind k)
{
    if (const auto* p = std::get_if<domain::DownloadBackendKind>(&r)) {
        return *p == k;
    }
    return false;
}

bool isErr(const BackendSelectionResult& r,
    BackendSelectionError::Kind k)
{
    if (const auto* e = std::get_if<BackendSelectionError>(&r)) {
        return e->kind == k;
    }
    return false;
}

} // namespace

class TstBackendSelectionPolicy : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void rdActivePicksRd()
    {
        auto in = base();
        in.activeDebrid = domain::DebridProvider::RealDebrid;
        QVERIFY(isKind(selectBackend(in),
            domain::DownloadBackendKind::RealDebridHttp));
    }

    void adActivePicksAd()
    {
        auto in = base();
        in.activeDebrid = domain::DebridProvider::AllDebrid;
        QVERIFY(isKind(selectBackend(in),
            domain::DownloadBackendKind::AllDebridHttp));
    }

    void noneActivePicksTorrent()
    {
        auto in = base();
        in.activeDebrid = domain::DebridProvider::None;
        QVERIFY(isKind(selectBackend(in),
            domain::DownloadBackendKind::Torrent));
    }

    void activeProviderUnsupportedSurfacesError()
    {
        auto in = base();
        in.activeDebrid = domain::DebridProvider::RealDebrid;
        for (auto& b : in.backends) {
            if (b.kind == domain::DownloadBackendKind::RealDebridHttp) {
                b.canHandle = false;
            }
        }
        QVERIFY(isErr(selectBackend(in),
            BackendSelectionError::Kind::ActiveProviderUnsupported));
    }

    void overrideHonoured()
    {
        auto in = base();
        in.activeDebrid = domain::DebridProvider::RealDebrid;
        in.override = domain::DownloadBackendKind::Torrent;
        QVERIFY(isKind(selectBackend(in),
            domain::DownloadBackendKind::Torrent));
    }

    void overrideUnsupportedSurfacesError()
    {
        auto in = base();
        in.override = domain::DownloadBackendKind::Torrent;
        for (auto& b : in.backends) {
            if (b.kind == domain::DownloadBackendKind::Torrent) {
                b.canHandle = false;
            }
        }
        QVERIFY(isErr(selectBackend(in),
            BackendSelectionError::Kind::OverrideUnsupported));
    }

    void torrentFallbackWhenNoDebrid()
    {
        auto in = base();
        // No active debrid, torrent should be picked.
        QVERIFY(isKind(selectBackend(in),
            domain::DownloadBackendKind::Torrent));
    }

    void noBackendError()
    {
        BackendSelectionInputs in;
        in.stream = makeStream();
        in.backends = {
            { domain::DownloadBackendKind::Torrent, false },
        };
        QVERIFY(isErr(selectBackend(in),
            BackendSelectionError::Kind::NoBackend));
    }
};

QTEST_MAIN(TstBackendSelectionPolicy)
#include "tst_backend_selection_policy.moc"
