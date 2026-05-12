// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"

#include <QString>

namespace kinema::download::picker {

/// One row passed to `chooseBest`. Both debrid resolvers flatten
/// their provider-specific file list into this shape before scoring.
struct Candidate {
    QString path; ///< Full path inside the torrent (folders joined by `/`).
    qint64 bytes = 0;
};

/// Score a candidate against the asset hint. Higher is better.
/// Used by both `RealDebridResolver` and `AllDebridResolver`.
int score(const QString& path, qint64 sizeBytes, const domain::AssetRef& ref);

/// Pick the index (into `candidates`) of the best file for `ref`,
/// or -1 if `candidates` is empty. When two candidates tie on score
/// the larger one wins.
int chooseIndex(const QList<Candidate>& candidates, const domain::AssetRef& ref);

} // namespace kinema::download::picker
