// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"
#include "core/DateWindow.h"

#include <KSharedConfig>

#include <QList>
#include <QObject>

namespace kinema::config {

/**
 * Browse page filter-bar state. Write-through, no signals — the Browse
 * page drives its own refetches when the user changes controls.
 *
 * KConfig group: [Browse]
 * Keys:
 *   kind          "Movie" | "Series"
 *   genreIds      CSV of TMDB genre ids
 *   dateWindow    "month1"|"month3"|"year"|"year3"|"any"
 *   minRatingPct  0 | 60 | 70 | 75 | 80 (rating * 10)
 *   sort          "popularity"|"releaseDate"|"rating"|"title"
 *   hideObscure   bool (maps to vote_count.gte=200)
 */
class BrowseSettings : public QObject
{
    Q_OBJECT
public:
    explicit BrowseSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    api::MediaKind kind() const;
    void setKind(api::MediaKind);

    QList<int> genreIds() const;
    void setGenreIds(QList<int>);

    core::DateWindow dateWindow() const;
    void setDateWindow(core::DateWindow);

    /// Minimum rating expressed as rating * 10 (0 = any).
    int minRatingPct() const;
    void setMinRatingPct(int);

    api::DiscoverSort sort() const;
    void setSort(api::DiscoverSort);

    bool hideObscure() const;
    void setHideObscure(bool);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
