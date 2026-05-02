// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/QueueDisplay.h"

#include <KLocalizedString>

namespace kinema::ui::qml::queue_display {

QString previewTitle(const api::QueueItem& item)
{
    return item.seriesTitle.isEmpty() ? item.title : item.seriesTitle;
}

QString subtitle(const api::QueueItem& item)
{
    if (item.key.kind != api::MediaKind::Series) {
        return {};
    }
    if (item.key.season && item.key.episode) {
        const auto sxe = QStringLiteral("S%1E%2")
                             .arg(*item.key.season,
                                 /*fieldWidth=*/2, /*base=*/10,
                                 QLatin1Char('0'))
                             .arg(*item.key.episode,
                                 /*fieldWidth=*/2, /*base=*/10,
                                 QLatin1Char('0'));
        if (!item.episodeTitle.isEmpty()) {
            return i18nc("@label queue row episode subtitle, "
                         "%1 SxE, %2 episode title",
                "%1 - %2", sxe, item.episodeTitle);
        }
        return sxe;
    }
    return item.episodeTitle;
}

QStringList previewChips(const api::QueueItem& item)
{
    QStringList chips;

    const QString resolution = item.streamRef.resolution.trimmed();
    const QString quality = item.streamRef.qualityLabel.trimmed();
    const QString provider = item.streamRef.provider.trimmed();

    if (!resolution.isEmpty()) {
        chips.append(resolution.toUpper());
    }
    if (!quality.isEmpty()
        && quality.compare(resolution, Qt::CaseInsensitive) != 0) {
        chips.append(quality);
    }
    if (!provider.isEmpty()) {
        chips.append(provider);
    }

    return chips;
}

} // namespace kinema::ui::qml::queue_display
