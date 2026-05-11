// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Indexer.h"

namespace kinema::api {

QString indexerKindToString(IndexerKind k)
{
    switch (k) {
    case IndexerKind::Torrentio:
        return QStringLiteral("torrentio");
    case IndexerKind::Peerflix:
        return QStringLiteral("peerflix");
    }
    return QStringLiteral("torrentio");
}

IndexerKind indexerKindFromString(const QString& s)
{
    const auto v = s.trimmed().toLower();
    if (v == QLatin1String("peerflix")) {
        return IndexerKind::Peerflix;
    }
    // Default — covers "torrentio", empty, and anything unknown.
    return IndexerKind::Torrentio;
}

Indexer::Indexer(QObject* parent)
    : QObject(parent)
{
}

Indexer::~Indexer() = default;

QCoro::Task<bool> Indexer::testConnection()
{
    try {
        // The Matrix (1999) — a stable, popular IMDB id that every
        // public indexer worth its salt has hits for.
        (void)co_await streams(MediaKind::Movie, QStringLiteral("tt0133093"));
        co_return true;
    } catch (...) {
        co_return false;
    }
}

} // namespace kinema::api
