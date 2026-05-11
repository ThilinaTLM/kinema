// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/StreamSorting.h"

#include "core/StreamTokens.h"

#include <algorithm>

namespace kinema::ui::qml::stream_sorting {

int resolutionRank(const QString& res)
{
    if (res == QLatin1String("2160p")) return 2160;
    if (res == QLatin1String("1440p")) return 1440;
    if (res == QLatin1String("1080p")) return 1080;
    if (res == QLatin1String("720p"))  return 720;
    if (res == QLatin1String("480p"))  return 480;
    if (res == QLatin1String("360p"))  return 360;
    return 0;
}

bool UiFilters::any() const noexcept
{
    return !resolution.isEmpty()
        || hdrOnly || dolbyVisionOnly || multiAudioOnly;
}

QList<api::Stream> applyUiFilters(QList<api::Stream> rows,
    const UiFilters& filters)
{
    if (!filters.any()) {
        return rows;
    }

    QList<api::Stream> out;
    out.reserve(rows.size());
    for (auto& s : rows) {
        if (!filters.resolution.isEmpty()) {
            const auto& res = s.resolution;
            if (filters.resolution == QStringLiteral("sd")) {
                // "SD" bucket = anything below 720p (incl. unknown).
                if (res == QStringLiteral("2160p") || res == QStringLiteral("1440p")
                    || res == QStringLiteral("1080p") || res == QStringLiteral("720p")) {
                    continue;
                }
            } else if (res != filters.resolution) {
                continue;
            }
        }
        if (filters.hdrOnly || filters.dolbyVisionOnly || filters.multiAudioOnly) {
            const auto t = core::stream_tokens::parse(s);
            if (filters.dolbyVisionOnly
                && t.hdr != core::stream_tokens::Hdr::DolbyVision) {
                continue;
            }
            if (filters.hdrOnly
                && t.hdr == core::stream_tokens::Hdr::Sdr) {
                continue;
            }
            if (filters.multiAudioOnly && !t.multiAudio) {
                continue;
            }
        }
        out.append(std::move(s));
    }
    return out;
}

void sortInPlace(QList<api::Stream>& rows,
    StreamsListModel::SortMode mode, bool descending)
{
    using SortMode = StreamsListModel::SortMode;

    if (mode == SortMode::Smart) {
        // Grouped by resolution descending, then seeders descending
        // within each quality group. "Smart" has a fixed shape; the
        // descending toggle is ignored.
        std::stable_sort(rows.begin(), rows.end(),
            [](const api::Stream& a, const api::Stream& b) {
                const int aRes = resolutionRank(a.resolution);
                const int bRes = resolutionRank(b.resolution);
                if (aRes != bRes) return aRes > bRes;
                const int aSeeders = a.seeders.value_or(-1);
                const int bSeeders = b.seeders.value_or(-1);
                if (aSeeders != bSeeders) return aSeeders > bSeeders;
                return a.sizeBytes.value_or(-1)
                    > b.sizeBytes.value_or(-1);
            });
        return;
    }

    auto cmp = [mode](const api::Stream& a, const api::Stream& b) {
        switch (mode) {
        case SortMode::Smart:
            return false; // unreachable
        case SortMode::Seeders:
            return a.seeders.value_or(-1) < b.seeders.value_or(-1);
        case SortMode::Size:
            return a.sizeBytes.value_or(-1) < b.sizeBytes.value_or(-1);
        case SortMode::Quality:
            return resolutionRank(a.resolution)
                < resolutionRank(b.resolution);
        case SortMode::Provider:
            return a.provider.localeAwareCompare(b.provider) < 0;
        case SortMode::ReleaseName:
            return a.releaseName.localeAwareCompare(b.releaseName) < 0;
        }
        return false;
    };
    if (descending) {
        std::stable_sort(rows.begin(), rows.end(),
            [cmp](const api::Stream& a, const api::Stream& b) {
                return cmp(b, a);
            });
    } else {
        std::stable_sort(rows.begin(), rows.end(), cmp);
    }
}

} // namespace kinema::ui::qml::stream_sorting
