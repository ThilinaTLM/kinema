// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/AllDebridResolver.h"

#include "api/AllDebridClient.h"
#include "core/io/HttpError.h"
#include "core/util/Magnet.h"
#include "playback/sources/DebridFilePicker.h"
#include "playback/sources/DebridResolverUtil.h"

#include <KLocalizedString>

namespace kinema::playback::sources {

namespace {

constexpr int kReadyTimeoutMs = 90'000;

bool isTerminalErrorStatus(int statusCode)
{
    // Per AllDebrid docs: codes 5..15 are terminal errors.
    return statusCode >= 5 && statusCode <= 15;
}

bool isReadyStatus(int statusCode)
{
    // 4 = Ready. We treat 2 (Compressing/Moving) and 3 (Uploading)
    // as not-yet-ready because file links aren't published until 4.
    return statusCode == 4;
}

} // namespace

AllDebridResolver::AllDebridResolver(api::AllDebridClient& ad,
    QObject* parent)
    : DebridResolver(parent)
    , m_ad(ad)
{
}

QCoro::Task<ResolvedDebridLink> AllDebridResolver::resolve(domain::AssetRef ref)
{
    requireInfoHash(ref, QStringLiteral("AllDebrid"));

    // Step 1: upload the magnet.
    const auto magnet = core::magnet::build(ref.infoHash, ref.releaseName);
    const auto added = co_await m_ad.uploadMagnet(magnet);

    // Step 2: poll until Ready or terminal error or timeout.
    domain::AdMagnetStatus status;
    PollBudget budget(kReadyTimeoutMs);
    while (true) {
        status = co_await m_ad.magnetStatus(added.id);
        if (isReadyStatus(status.statusCode)) {
            break;
        }
        if (isTerminalErrorStatus(status.statusCode)) {
            const auto label = status.status.isEmpty()
                ? i18n("error")
                : status.status;
            throw core::HttpError(core::HttpError::Kind::HttpStatus, 502,
                i18n("AllDebrid magnet failed (status %1: %2).",
                    QString::number(status.statusCode), label));
        }
        if (budget.expired()) {
            throw core::HttpError(core::HttpError::Kind::HttpStatus, 504,
                i18n("AllDebrid did not produce a ready magnet in time."));
        }
        co_await budget.tick();
    }

    // Step 3: list the files.
    const auto files = co_await m_ad.magnetFiles(added.id);
    if (files.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid did not return any files for the magnet."));
    }

    // Step 4: pick the best file via the shared scoring helper.
    QList<picker::Candidate> candidates;
    candidates.reserve(files.size());
    for (const auto& f : files) {
        candidates.append({ f.path, f.bytes });
    }
    const int idx = picker::chooseIndex(candidates, ref);
    if (idx < 0) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid did not return a usable file."));
    }
    const auto& chosen = files[idx];

    // Step 5: unlock the hoster link. The client handles the
    // `delayed` flow internally.
    const auto unlocked = co_await m_ad.unlockLink(chosen.hosterLink);

    ResolvedDebridLink out;
    out.downloadUrl = unlocked.download;
    out.fileSize = unlocked.fileSize > 0
        ? unlocked.fileSize
        : chosen.bytes;
    out.fileName = unlocked.filename.isEmpty()
        ? (chosen.path.isEmpty() ? ref.fileNameHint : chosen.path)
        : unlocked.filename;

    // Preserve the full magnet file list so the asset session can
    // surface it to series auto-next without a libtorrent session.
    out.files = flattenFileList(files);
    co_return out;
}

} // namespace kinema::playback::sources
