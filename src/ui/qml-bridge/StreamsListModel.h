// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QAbstractListModel>
#include <QDate>
#include <QHash>
#include <QList>
#include <QString>
#include <QStringList>
#include <QVariant>

namespace kinema::ui::qml {

/**
 * QML-friendly list model for one detail page's torrent list.
 *
 * Replaces the widget-side `ui::TorrentsModel` (`QAbstractTableModel`
 * with positional column roles) with a flat list of named roles
 * suitable for a `ListView`-of-`StreamCard`. Carries its own
 * lifecycle state (`Idle`/`Loading`/`Ready`/`Empty`/`Error`/`Unreleased`)
 * so the page can switch placeholders on `model.state` directly,
 * matching the `DiscoverSectionModel` / `ResultsListModel` shape.
 *
 * Sort order and cached-only / blocklist filtering live on the
 * owning view-model: it keeps the unfiltered raw list, applies
 * filters + sort, and calls `setItems(visible)` with the rendered
 * result. The model itself is intentionally dumb — it just holds
 * what's currently visible plus the state/error/release-date side
 * channels the page needs to render placeholders.
 *
 * The `SortMode` enum is the canonical client-side sort axis. It
 * is distinct from `core::torrentio::SortMode` (the wire-format
 * URL parameter, which has only three values); the UI's sort menu
 * is richer than what Torrentio knows how to do server-side.
 */
class StreamsListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(State state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString errorMessage READ errorMessage NOTIFY errorMessageChanged)
    Q_PROPERTY(QString emptyExplanation READ emptyExplanation NOTIFY emptyExplanationChanged)
    Q_PROPERTY(QDate releaseDate READ releaseDate NOTIFY releaseDateChanged)
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)

public:
    enum Roles {
        StreamRole = Qt::UserRole + 1, ///< full api::Stream as QVariant
        ReleaseNameRole,
        DetailsTextRole,
        ResolutionRole,
        QualityLabelRole, ///< resolution + " · [RD+]" / " · [RD]"
        SizeBytesRole,    ///< qint64; -1 if unknown
        SizeTextRole,     ///< localized "1.2 GB" / "—"
        SeedersRole,      ///< int; -1 if unknown
        ProviderRole,
        InfoHashRole,
        DirectUrlRole,    ///< QString (empty if RD didn't resolve one)
        RdCachedRole,
        RdDownloadRole,
        HasMagnetRole,
        HasDirectUrlRole,
        ChipsRole,        ///< QStringList of small label pills
    };
    Q_ENUM(Roles)

    /// Lifecycle states the streams area renders. The owning page
    /// binds a StackLayout (or Loader) to switch on these.
    enum class State {
        Idle = 0,    ///< nothing loaded yet
        Loading,
        Ready,       ///< at least one visible row
        Empty,       ///< loaded but no visible rows (filtered or upstream empty)
        Error,
        Unreleased,  ///< title's release date is in the future
    };
    Q_ENUM(State)

    /// Client-side sort axes. Values carry no semantic meaning; the
    /// view-model maps each to a comparator over `api::Stream`.
    enum class SortMode {
        Seeders = 0,
        Size,
        Quality,
        Provider,
        ReleaseName,
    };
    Q_ENUM(SortMode)

    explicit StreamsListModel(QObject* parent = nullptr);

    State state() const noexcept { return m_state; }
    QString errorMessage() const { return m_errorMessage; }
    QString emptyExplanation() const { return m_emptyExplanation; }
    QDate releaseDate() const { return m_releaseDate; }

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Mutators driven by the view-model.
    void setIdle();
    void setLoading();
    /// Replace the visible row list; flips state to Ready/Empty.
    /// `emptyExplanation` is shown when the resulting state is Empty;
    /// pass an empty string for the default placeholder body.
    void setItems(QList<api::Stream> visible,
        const QString& emptyExplanation = {});
    void setError(const QString& message);
    void setUnreleased(const QDate& date);

    /// Pure accessors for the view-model and tests.
    const QList<api::Stream>& items() const noexcept { return m_items; }
    const api::Stream* at(int row) const;

    /// Localized "1.2 GB" / "—". Exposed publicly so the view-model
    /// can mirror the same formatting in chip strings or the page
    /// header without duplicating the qint64 → string logic.
    static QString formatSize(std::optional<qint64> bytes);

    /// Build the per-row chip list (resolution + RD flag + provider
    /// + extras parsed from the details line). Pure helper so unit
    /// tests can poke it.
    static QStringList chipsFor(const api::Stream& s);

Q_SIGNALS:
    void stateChanged();
    void errorMessageChanged();
    void emptyExplanationChanged();
    void releaseDateChanged();
    void countChanged();

private:
    void resetState(State newState);

    QList<api::Stream> m_items;
    State m_state = State::Idle;
    QString m_errorMessage;
    QString m_emptyExplanation;
    QDate m_releaseDate;
};

} // namespace kinema::ui::qml
