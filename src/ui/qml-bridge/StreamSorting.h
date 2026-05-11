// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "ui/qml-bridge/StreamsListModel.h"

#include <QList>
#include <QString>

namespace kinema::ui::qml::stream_sorting {

/// "1080p" / "2160p" → numeric height. Larger sorts higher; missing /
/// unknown becomes 0. Used by the detail VMs to compare streams by
/// resolution without re-parsing the label.
int resolutionRank(const QString& res);

/// Transient (page-scoped) UI filters surfaced by the
/// `StreamsPage`'s ActionToolBar. Persistent filters
/// (cached-only / keyword blocklist) stay on `core::stream_filter`.
struct UiFilters {
    QString resolution; ///< "" | "2160p" | "1080p" | "720p" | "sd"
    bool hdrOnly = false;
    bool dolbyVisionOnly = false;
    bool multiAudioOnly = false;

    bool any() const noexcept;
};

/// Drop rows that fail the transient UI filters. `rows` is consumed
/// by move when none of the filters are active.
QList<domain::Stream> applyUiFilters(QList<domain::Stream> rows,
    const UiFilters& filters);

/// Sort `rows` in place by `mode`. Stable. `Smart` ignores
/// `descending` (it has a fixed shape: cached → resolution → seeders).
void sortInPlace(QList<domain::Stream>& rows,
    StreamsListModel::SortMode mode,
    bool descending);

} // namespace kinema::ui::qml::stream_sorting
